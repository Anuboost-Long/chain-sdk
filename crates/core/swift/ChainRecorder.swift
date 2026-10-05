// Core Audio capture bridge for crates/core/src/audio_recorder.rs — see
// agent-docs/capabilities/audio-recorder/research/MACOS.md. Records an
// input device, a global process tap (the computer's own output), or both
// through one private aggregate device, and hands Rust mono f32 samples
// per source. If the device disappears mid-recording the aggregate is
// rebuilt on another one. Only numbers and C strings (JSON) cross the
// boundary.

import AVFoundation
import CoreAudio
import Foundation

public typealias ChainRecorderSamples = @convention(c) (
    UnsafeMutableRawPointer?, UnsafePointer<Float>?, UnsafePointer<Float>?, UInt32, Double
) -> Void
/// The recording moved to another microphone: JSON `{ microphone, previous, bluetoothFallback }`.
public typealias ChainRecorderMicrophoneChange = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>) -> Void

// Start status codes — keep in sync with audio_recorder.rs's `native` module.
private let statusOk: Int32 = 0
private let statusUnsupported: Int32 = 1
private let statusDenied: Int32 = 2
private let statusFailed: Int32 = 3
private let statusUnavailable: Int32 = 4

private struct Failure: Error {
    let status: Int32
    let message: String
}

private func check(_ status: OSStatus, _ what: String) throws {
    if status != noErr {
        throw Failure(status: statusFailed, message: "couldn't \(what) (OSStatus \(status))")
    }
}

private func declares(_ key: String) -> Bool {
    (Bundle.main.object(forInfoDictionaryKey: key) as? String)?.isEmpty == false
}

private func address(_ selector: AudioObjectPropertySelector, _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal)
    -> AudioObjectPropertyAddress
{
    AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain)
}

private func defaultDevice(_ selector: AudioObjectPropertySelector) -> AudioObjectID {
    var property = address(selector)
    var device = AudioObjectID(kAudioObjectUnknown)
    var size = UInt32(MemoryLayout<AudioObjectID>.size)
    let status = AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &property, 0, nil, &size, &device)
    return status == noErr ? device : AudioObjectID(kAudioObjectUnknown)
}

private func deviceUID(_ device: AudioObjectID) throws -> String {
    var property = address(kAudioDevicePropertyDeviceUID)
    var uid: Unmanaged<CFString>?
    var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
    try check(AudioObjectGetPropertyData(device, &property, 0, nil, &size, &uid), "read the audio device's id")
    guard let uid else { throw Failure(status: statusFailed, message: "the audio device has no id") }
    return uid.takeRetainedValue() as String
}

private func inputBufferCount(_ device: AudioObjectID) -> Int {
    var property = address(kAudioDevicePropertyStreamConfiguration, kAudioObjectPropertyScopeInput)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(device, &property, 0, nil, &size) == noErr, size > 0 else { return 0 }
    let raw = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: MemoryLayout<AudioBufferList>.alignment)
    defer { raw.deallocate() }
    guard AudioObjectGetPropertyData(device, &property, 0, nil, &size, raw) == noErr else { return 0 }
    return Int(raw.assumingMemoryBound(to: AudioBufferList.self).pointee.mNumberBuffers)
}

private func nominalSampleRate(_ device: AudioObjectID) throws -> Double {
    var property = address(kAudioDevicePropertyNominalSampleRate)
    var rate: Double = 0
    var size = UInt32(MemoryLayout<Double>.size)
    try check(AudioObjectGetPropertyData(device, &property, 0, nil, &size, &rate), "read the sample rate")
    return rate
}

private func stringProperty(_ device: AudioObjectID, _ selector: AudioObjectPropertySelector) -> String? {
    var property = address(selector)
    var value: Unmanaged<CFString>?
    var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
    guard AudioObjectGetPropertyData(device, &property, 0, nil, &size, &value) == noErr, let value else { return nil }
    return value.takeRetainedValue() as String
}

private func transportType(_ device: AudioObjectID) -> UInt32 {
    var property = address(kAudioDevicePropertyTransportType)
    var transport: UInt32 = 0
    var size = UInt32(MemoryLayout<UInt32>.size)
    return AudioObjectGetPropertyData(device, &property, 0, nil, &size, &transport) == noErr ? transport : 0
}

private func bufferFrameSize(_ device: AudioObjectID) -> Int {
    var property = address(kAudioDevicePropertyBufferFrameSize)
    var frames: UInt32 = 0
    var size = UInt32(MemoryLayout<UInt32>.size)
    return AudioObjectGetPropertyData(device, &property, 0, nil, &size, &frames) == noErr ? Int(frames) : 0
}

// MARK: Microphones

/// Our own aggregates' UIDs start with this, so they're never listed as inputs.
private let aggregatePrefix = "chain-recorder-"

private struct Input {
    let device: AudioObjectID
    let uid: String
    let name: String
    let transport: String
    let sampleRate: Double

    var isBluetooth: Bool { transport == "bluetooth" }

    func json(isDefault: Bool) -> [String: Any] {
        ["id": uid, "name": name, "transport": transport, "isDefault": isDefault, "sampleRate": sampleRate]
    }

    var json: [String: Any] { json(isDefault: device == defaultDevice(kAudioHardwarePropertyDefaultInputDevice)) }
}

private func transportName(_ transport: UInt32) -> String {
    switch transport {
    case kAudioDeviceTransportTypeBuiltIn: "built-in"
    case kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE: "bluetooth"
    case kAudioDeviceTransportTypeUSB: "usb"
    default: "other"
    }
}

private func inputs() -> [Input] {
    var property = address(kAudioHardwarePropertyDevices)
    var size: UInt32 = 0
    let system = AudioObjectID(kAudioObjectSystemObject)
    guard AudioObjectGetPropertyDataSize(system, &property, 0, nil, &size) == noErr else { return [] }
    var devices = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(system, &property, 0, nil, &size, &devices) == noErr else { return [] }
    return devices.compactMap { device in
        guard inputBufferCount(device) > 0,
              let uid = stringProperty(device, kAudioDevicePropertyDeviceUID),
              !uid.hasPrefix(aggregatePrefix)
        else { return nil }
        return Input(
            device: device,
            uid: uid,
            name: stringProperty(device, kAudioObjectPropertyName) ?? "Microphone",
            transport: transportName(transportType(device)),
            sampleRate: (try? nominalSampleRate(device)) ?? 0
        )
    }
}

/// The input to use when Bluetooth is avoided: the built-in microphone,
/// else the default if it isn't Bluetooth, else any other non-Bluetooth
/// input, else Bluetooth after all (`fallback`).
private func avoidingBluetooth(_ all: [Input]) -> (input: Input, fallback: Bool)? {
    let fallback = all.first { $0.device == defaultDevice(kAudioHardwarePropertyDefaultInputDevice) } ?? all.first
    if let other = all.first(where: { $0.transport == "built-in" })
        ?? fallback.flatMap({ $0.isBluetooth ? nil : $0 })
        ?? all.first(where: { !$0.isBluetooth })
    {
        return (other, false)
    }
    return fallback.map { ($0, true) }
}

private func chooseInput(uid: String?, avoidBluetooth: Bool) throws -> (input: Input, fallback: Bool) {
    let all = inputs()
    if let uid {
        guard let chosen = all.first(where: { $0.uid == uid }) else {
            throw Failure(status: statusUnavailable, message: "that microphone isn't connected")
        }
        return (chosen, false)
    }
    let defaultInput = defaultDevice(kAudioHardwarePropertyDefaultInputDevice)
    guard let current = all.first(where: { $0.device == defaultInput }) ?? all.first else {
        throw Failure(status: statusFailed, message: "there's no input device")
    }
    if avoidBluetooth, current.isBluetooth, let other = avoidingBluetooth(all) {
        return other
    }
    return (current, false)
}

private func jsonString(_ value: Any) -> String {
    guard let data = try? JSONSerialization.data(withJSONObject: value) else { return "null" }
    return String(decoding: data, as: UTF8.self)
}

@_cdecl("chain_recorder_microphones")
public func chainRecorderMicrophones() -> UnsafeMutablePointer<CChar>? {
    strdup(jsonString(inputs().map(\.json)))
}

// MARK: Permissions

private final class Answer: @unchecked Sendable {
    var granted = false
    let done = DispatchSemaphore(value: 0)
}

private func microphoneGranted() -> Bool {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized:
        return true
    case .notDetermined:
        let answer = Answer()
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            answer.granted = granted
            answer.done.signal()
        }
        answer.done.wait()
        return answer.granted
    default:
        return false
    }
}

// System audio capture has no public authorization API, and a refused tap
// records silence rather than failing — so ask TCC directly. Missing
// symbols (a future macOS) mean we can't tell; the tap then prompts itself.
private typealias TCCPreflight = @convention(c) (CFString, CFDictionary?) -> Int
private typealias TCCRequest = @convention(c) (CFString, CFDictionary?, @escaping @convention(block) (Bool) -> Void) -> Void

private func systemAudioGranted() -> Bool {
    let service = "kTCCServiceAudioCapture" as CFString
    guard let tcc = dlopen("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC", RTLD_NOW),
          let preflightSymbol = dlsym(tcc, "TCCAccessPreflight"),
          let requestSymbol = dlsym(tcc, "TCCAccessRequest")
    else { return true }
    let preflight = unsafeBitCast(preflightSymbol, to: TCCPreflight.self)
    switch preflight(service, nil) {
    case 0:
        return true
    case 1:
        return false
    default:
        let request = unsafeBitCast(requestSymbol, to: TCCRequest.self)
        let answer = Answer()
        request(service, nil) { granted in
            answer.granted = granted
            answer.done.signal()
        }
        answer.done.wait()
        return answer.granted
    }
}

// MARK: Availability

@_cdecl("chain_recorder_microphone_available")
public func chainRecorderMicrophoneAvailable() -> Bool {
    declares("NSMicrophoneUsageDescription")
        && defaultDevice(kAudioHardwarePropertyDefaultInputDevice) != kAudioObjectUnknown
}

@_cdecl("chain_recorder_system_available")
public func chainRecorderSystemAvailable() -> Bool {
    guard #available(macOS 14.2, *) else { return false }
    return declares("NSAudioCaptureUsageDescription")
}

// MARK: Recording

private final class Session: @unchecked Sendable {
    let microphone: Bool
    let system: Bool
    let context: UnsafeMutableRawPointer?
    let onSamples: ChainRecorderSamples
    let onMicrophoneChange: ChainRecorderMicrophoneChange
    var tap = AudioObjectID(kAudioObjectUnknown)
    var tapUID: String?
    var tapBuffers = 0
    var aggregate = AudioObjectID(kAudioObjectUnknown)
    var ioProc: AudioDeviceIOProcID?
    /// The aggregate's clock: the input for microphone and both, else the output.
    var clock = AudioObjectID(kAudioObjectUnknown)
    var input: Input?
    let queue = DispatchQueue(label: "chain.audio-recorder", qos: .userInitiated)
    let watch = DispatchQueue(label: "chain.audio-recorder.devices")
    var devicesListener: AudioObjectPropertyListenerBlock?
    var microphoneSamples: [Float] = []
    var systemSamples: [Float] = []

    init(microphone: Bool, system: Bool, context: UnsafeMutableRawPointer?, onSamples: ChainRecorderSamples,
         onMicrophoneChange: ChainRecorderMicrophoneChange)
    {
        self.microphone = microphone
        self.system = system
        self.context = context
        self.onSamples = onSamples
        self.onMicrophoneChange = onMicrophoneChange
    }

    func createTap() throws {
        guard #available(macOS 14.2, *) else { return }
        let description = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
        description.uuid = UUID()
        description.isPrivate = true
        description.muteBehavior = .unmuted
        try check(AudioHardwareCreateProcessTap(description, &tap), "create the system audio tap")
        tapUID = description.uuid.uuidString
        var format = AudioStreamBasicDescription()
        var property = address(kAudioTapPropertyFormat)
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        try check(AudioObjectGetPropertyData(tap, &property, 0, nil, &size, &format), "read the tap's format")
        let interleaved = format.mFormatFlags & kAudioFormatFlagIsNonInterleaved == 0
        tapBuffers = interleaved ? 1 : Int(format.mChannelsPerFrame)
    }

    /// Builds the aggregate on `clock` (plus the tap) and starts it.
    func open(clock: AudioObjectID) throws {
        let clockUID = try deviceUID(clock)
        var description: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Chain recorder",
            kAudioAggregateDeviceUIDKey: aggregatePrefix + UUID().uuidString,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceMainSubDeviceKey: clockUID,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: clockUID]],
        ]
        if let tapUID {
            description[kAudioAggregateDeviceTapAutoStartKey] = false
            description[kAudioAggregateDeviceTapListKey] = [
                [kAudioSubTapUIDKey: tapUID, kAudioSubTapDriftCompensationKey: true],
            ]
        }
        try check(AudioHardwareCreateAggregateDevice(description as CFDictionary, &aggregate), "create the recording device")
        self.clock = clock
        let sampleRate = try nominalSampleRate(aggregate)
        let cycleFrames = bufferFrameSize(aggregate)
        let microphoneBuffers = microphone ? inputBufferCount(clock) : 0
        let (microphone, system, tapBuffers) = (self.microphone, self.system, self.tapBuffers)
        try check(
            AudioDeviceCreateIOProcIDWithBlock(&ioProc, aggregate, queue) { [unowned self] _, inputData, _, _, _ in
                let buffers = Array(UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: inputData)))
                // The tap delivers empty buffers while nothing plays; that
                // time is still recorded, as silence.
                let frames = buffers.map(frameCount).max().flatMap { $0 > 0 ? $0 : nil } ?? cycleFrames
                guard frames > 0 else { return }
                if microphone {
                    downmix(buffers.prefix(microphoneBuffers), frames: frames, into: &self.microphoneSamples)
                }
                if system {
                    downmix(buffers.suffix(tapBuffers), frames: frames, into: &self.systemSamples)
                }
                self.microphoneSamples.withUnsafeBufferPointer { mic in
                    self.systemSamples.withUnsafeBufferPointer { sys in
                        self.onSamples(
                            self.context,
                            microphone ? mic.baseAddress : nil,
                            system ? sys.baseAddress : nil,
                            UInt32(frames),
                            sampleRate
                        )
                    }
                }
            },
            "attach to the recording device"
        )
        try check(AudioDeviceStart(aggregate, ioProc), "start recording")
    }

    /// Stops and destroys the aggregate, keeping the tap for a rebuild.
    func close() {
        if let ioProc {
            AudioDeviceStop(aggregate, ioProc)
            AudioDeviceDestroyIOProcID(aggregate, ioProc)
            // A callback already queued still runs; wait it out so Rust can
            // free the context once this returns.
            queue.sync {}
        }
        ioProc = nil
        if aggregate != kAudioObjectUnknown {
            AudioHardwareDestroyAggregateDevice(aggregate)
        }
        aggregate = AudioObjectID(kAudioObjectUnknown)
    }

    /// Default-device changes aren't followed — headphones connecting don't
    /// move the take. Only the clock device disappearing does.
    func watchDevices() {
        var property = address(kAudioHardwarePropertyDevices)
        let listener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in self?.devicesChanged() }
        if AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &property, watch, listener) == noErr {
            devicesListener = listener
        }
    }

    private func devicesChanged() {
        lock.lock()
        defer { lock.unlock() }
        guard current === self, aggregate != kAudioObjectUnknown, !isAlive(clock) else { return }
        close()
        do {
            if microphone {
                guard let previous = input, let replacement = avoidingBluetooth(inputs()) else {
                    throw Failure(status: statusFailed, message: "no microphone left")
                }
                try open(clock: replacement.input.device)
                input = replacement.input
                let change = jsonString([
                    "microphone": replacement.input.json,
                    "previous": previous.json(isDefault: false),
                    "bluetoothFallback": replacement.fallback,
                ])
                change.withCString { onMicrophoneChange(context, $0) }
            } else {
                try open(clock: defaultDevice(kAudioHardwarePropertyDefaultOutputDevice))
            }
        } catch {
            // Nothing to record from; stop() still cleans up.
            close()
        }
    }

    func stop() {
        if let devicesListener {
            var property = address(kAudioHardwarePropertyDevices)
            AudioObjectRemovePropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &property, watch, devicesListener)
        }
        devicesListener = nil
        close()
        if tap != kAudioObjectUnknown, #available(macOS 14.2, *) {
            AudioHardwareDestroyProcessTap(tap)
        }
        tap = AudioObjectID(kAudioObjectUnknown)
    }
}

private func isAlive(_ device: AudioObjectID) -> Bool {
    var property = address(kAudioDevicePropertyDeviceIsAlive)
    var alive: UInt32 = 0
    var size = UInt32(MemoryLayout<UInt32>.size)
    return AudioObjectGetPropertyData(device, &property, 0, nil, &size, &alive) == noErr && alive != 0
}

private let lock = NSLock()
nonisolated(unsafe) private var current: Session?

private func frameCount(_ buffer: AudioBuffer) -> Int {
    guard buffer.mNumberChannels > 0, buffer.mData != nil else { return 0 }
    return Int(buffer.mDataByteSize) / (MemoryLayout<Float>.size * Int(buffer.mNumberChannels))
}

/// Averages `buffers` (any mix of interleaved and planar channels) into
/// `mono`; frames a buffer doesn't carry count as silence.
private func downmix(_ buffers: ArraySlice<AudioBuffer>, frames: Int, into mono: inout [Float]) {
    if mono.count < frames { mono = [Float](repeating: 0, count: frames) }
    for i in 0..<frames { mono[i] = 0 }
    var channels = 0
    for buffer in buffers {
        let count = Int(buffer.mNumberChannels)
        guard count > 0, let data = buffer.mData?.assumingMemoryBound(to: Float.self) else { continue }
        channels += count
        for frame in 0..<min(frames, frameCount(buffer)) {
            var sum: Float = 0
            for channel in 0..<count { sum += data[frame * count + channel] }
            mono[frame] += sum
        }
    }
    if channels > 1 {
        let scale = 1 / Float(channels)
        for i in 0..<frames { mono[i] *= scale }
    }
}

/// Returns the session and the JSON `{ microphone, bluetoothFallback }`.
private func start(
    _ session: Session, microphoneUID: String?, avoidBluetooth: Bool
) throws -> String {
    if session.microphone {
        guard chainRecorderMicrophoneAvailable() else {
            throw Failure(status: statusUnsupported, message: "the app doesn't declare chain.permissions.microphone, or there's no input device")
        }
        guard microphoneGranted() else {
            throw Failure(status: statusDenied, message: "microphone access was refused")
        }
    }
    if session.system {
        guard chainRecorderSystemAvailable() else {
            throw Failure(status: statusUnsupported, message: "recording the computer's audio needs macOS 14.2 and chain.permissions.systemAudio")
        }
        guard systemAudioGranted() else {
            throw Failure(status: statusDenied, message: "computer audio recording access was refused")
        }
    }

    do {
        var started: [String: Any] = ["bluetoothFallback": false]
        let clock: AudioObjectID
        if session.microphone {
            let (input, fallback) = try chooseInput(uid: microphoneUID, avoidBluetooth: avoidBluetooth)
            session.input = input
            clock = input.device
            started = ["microphone": input.json, "bluetoothFallback": fallback]
        } else {
            clock = defaultDevice(kAudioHardwarePropertyDefaultOutputDevice)
            guard clock != kAudioObjectUnknown else {
                throw Failure(status: statusFailed, message: "there's no default output device")
            }
        }
        if session.system {
            try session.createTap()
        }
        try session.open(clock: clock)
        session.watchDevices()
        return jsonString(started)
    } catch {
        session.stop()
        throw error
    }
}

@_cdecl("chain_recorder_start")
public func chainRecorderStart(
    _ microphone: Bool,
    _ system: Bool,
    _ microphoneUID: UnsafePointer<CChar>?,
    _ avoidBluetooth: Bool,
    _ context: UnsafeMutableRawPointer?,
    _ onSamples: ChainRecorderSamples,
    _ onMicrophoneChange: ChainRecorderMicrophoneChange,
    _ started: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
    _ message: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
    lock.lock()
    defer { lock.unlock() }
    current?.stop()
    current = nil
    let session = Session(
        microphone: microphone, system: system, context: context, onSamples: onSamples, onMicrophoneChange: onMicrophoneChange
    )
    do {
        let json = try start(session, microphoneUID: microphoneUID.map { String(cString: $0) }, avoidBluetooth: avoidBluetooth)
        current = session
        started.pointee = strdup(json)
        return statusOk
    } catch let failure as Failure {
        message.pointee = strdup(failure.message)
        return failure.status
    } catch {
        message.pointee = strdup(error.localizedDescription)
        return statusFailed
    }
}

/// Once this returns no more samples or microphone changes arrive.
@_cdecl("chain_recorder_stop")
public func chainRecorderStop() {
    lock.lock()
    defer { lock.unlock() }
    current?.stop()
    current = nil
}
