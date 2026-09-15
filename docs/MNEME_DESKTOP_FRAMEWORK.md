# Chain Framework Journey — Mneme as Application #1

## Human + AI Working Notes

> This document explains what we are trying to build, why we are doing it this way, how the different pieces fit together, and how the human developer and AI agents should work together.
>
> This is intentionally written as a document that **both humans and AI agents can understand**.
>
> It is not only an agent instruction file.
>
> It is our shared understanding of the project.

## Project Names

```text
Mneme
= Application #1 and the first real product used to validate the architecture

Chain
= The cross-platform desktop framework

Chain SDK
= The public JavaScript / TypeScript API used by applications

Chain Core
= The Rust orchestration, bridge, permissions, and runtime-abstraction layer

Chain Runtime
= The replaceable desktop runtime beneath Chain Core; Tauri is used initially

Chain Modules
= Reusable capabilities such as Files, Audio, Clipboard, Window, and Notifications
```

Applications should depend on **Chain**, not directly on Tauri, Rust, Swift, .NET, or operating-system APIs.

Mneme is the first real application used to discover, validate, and refine Chain.

---

# 1. How We Got Here

Mneme started as an AI-powered desktop learning workspace.

The application is intended to let a student:

```text
Import course and LMS material
Organize courses, modules, lessons, exercises, and discussions
Write and edit rich notes
Import PDFs, images, screenshots, and other resources
Record lectures and voice notes
Transcribe audio
Use text-to-speech
Extract text from images
Ask AI to summarize, explain, reorganize, and transform material
Create reusable one-click AI actions
Allow an AI agent to work directly with the student's workspace
```

A React + TypeScript frontend is a strong fit for this application because Mneme needs a highly dynamic interface:

```text
Rich text editor
Course/module navigation
Notion-style blocks
Drag and drop
AI panels
Custom actions
Search
Dashboards
Interactive study tools
```

However, Mneme is **not** just a web application.

It also needs real desktop functionality.

Examples:

```text
Filesystem access
Native file dialogs
Clipboard
Microphone and audio capture
Native notifications
Secure storage
Native window behavior
Deep links
System information
File watching
Drag and drop
Potential local AI/model processes
Potential screen capture and accessibility integration
```

We first considered Electron.

Electron would solve many of these problems, but it also means shipping Chromium and depending heavily on the Electron/Node ecosystem.

That led to a larger experiment:

> Can we create an Expo-like desktop development experience that keeps a shared React/TypeScript UI while using a lightweight Rust runtime and true platform-native modules where needed?

The initial architecture becomes:

```text
                       MNEME

                 React / TypeScript
                        UI
                         │
                         ▼
                Chain SDK
                         │
                         ▼
                    Chain Core (Rust)
                         │
                         ▼
               Tauri Runtime Initially
                         │
                ┌────────┴────────┐
                ▼                 ▼
             macOS              Windows
          Swift / Obj-C        C# / .NET
          Apple APIs         WinRT / Win32
```

Linux may be added later.

The UI is shared.

The contract is shared.

The native implementation is allowed to remain native.

---

# 2. The Important Realization

While designing Mneme, we realized that we already need to solve:

> "How does JavaScript call native desktop functionality through one stable cross-platform API?"

Once we solve that correctly, there is no reason the solution has to belong only to Mneme.

For example, Mneme may need:

```ts
const file = await desktop.files.pick();
```

The underlying implementation may differ:

```text
desktop.files.pick()
        │
   ┌────┴────┐
   ▼         ▼
 macOS     Windows
   │         │
Swift      .NET/Rust
```

But the React application does not care.

It only understands:

```ts
const file = await desktop.files.pick();
```

Likewise:

```ts
await desktop.audio.startRecording();
await desktop.clipboard.readText();
await desktop.notifications.show(...);
```

That means `Files`, `Audio`, `Clipboard`, `Notifications`, and other capabilities can potentially become reusable framework modules.

This led to the larger idea:

> Mneme is the first real application used to discover and validate **Chain**, our reusable cross-platform desktop framework and SDK.

---

# 3. Build the Minimum Framework First

The previous idea of "do not build the framework first" still contains an important warning:

> Do not build a huge framework based on assumptions.

However, Mneme now deliberately depends on a small framework foundation.

Therefore our updated rule is:

> Build the **minimum framework skeleton first**, then let Mneme drive every additional capability.

We should build only enough foundation to prove the architecture:

```text
Repository knowledge system
+
Root AGENTS.md
+
Component memory pattern
+
Chain SDK boundary
+
Chain Core (Rust)
+
Tauri bootstrap
+
One capability
```

The first capability should be intentionally tiny:

```ts
desktop.platform.getInfo();
```

Once this complete path works:

```text
React
  ↓
Chain SDK
  ↓
Rust
  ↓
Native/runtime layer
  ↓
Rust
  ↓
Chain SDK
  ↓
React
```

we stop expanding the framework speculatively.

After that:

```text
Mneme needs something
        │
        ▼
Research the capability
        │
        ▼
Design common contract
        │
        ▼
Implement one or more native adapters
        │
        ▼
Use it in Mneme
        │
        ▼
Test real behavior
        │
        ▼
Refine abstraction
        │
        ▼
Reusable framework module?
```

The framework still **emerges from real applications**.

We are only building the minimum skeleton first so every later capability follows the same architecture.

---

# 4. Mneme Is Application #1

Mneme will naturally test capabilities such as:

```text
Platform/System information
Filesystem
Native file dialogs
Clipboard
Microphone
Audio recording
Audio playback
Notifications
Secure storage
Window behavior
Deep links
File watching
Drag and drop
Application lifecycle
Local process/model integration
```

Some are clearly generic.

For example:

```ts
desktop.platform.getInfo();
desktop.files.pick();
desktop.clipboard.readText();
desktop.audio.startRecording();
desktop.notifications.show(...);
```

Other functionality is probably Mneme-specific:

```ts
Mneme.importModule();
Mneme.prepareModule();
Mneme.createStudySession();
Mneme.agent.runAction();
```

We must keep these categories separate.

The framework provides desktop capabilities.

Mneme provides learning behavior.

---

# 5. Other Applications Will Test Other Areas

Mneme cannot realistically discover every useful desktop capability.

Other real applications should test areas Mneme does not need.

For example, Wallpache can test:

```text
Display management
Power events
Media sessions
Wallpaper rendering
Desktop layering
Always-on-desktop behavior
Multi-monitor behavior
```

Lazify can test:

```text
Processes
Terminal
Filesystem watching
Ports
Git integration
Tray
Global shortcuts
Development tools
```

A dedicated device test application could test:

```text
Camera
Bluetooth
USB
Screen capture
Speaker routing
Advanced microphone controls
```

Therefore:

```text
Mneme
   │
   ├── Files
   ├── Audio
   ├── Clipboard
   ├── Secure Storage
   └── Notifications

Wallpache
   │
   ├── Display
   ├── Power
   ├── Media
   └── Desktop Integration

Lazify
   │
   ├── Process
   ├── Terminal
   ├── Filesystem Watch
   └── Network

Device Test App
   │
   ├── Camera
   ├── Bluetooth
   ├── USB
   └── Screen Capture

             ↓

        Chain SDK
```

Chain is validated by multiple real applications.

Mneme is simply the first core experiment.

---

# 6. What a Contract Means

One concept that can become confusing is **contract**.

The simplest explanation is:

> The contract is the common API and behavior that all platform implementations agree to provide.

Suppose Mneme needs microphone recording.

Internally we may have:

```text
macOS
├── Swift
├── AVFoundation
├── CoreAudio
└── Apple permission behavior

Windows
├── C#
├── .NET / NativeAOT
├── WinRT / Windows audio APIs
└── Windows permission behavior
```

Our application should not care.

Instead we define something like:

```ts
interface AudioApi {
  startRecording(options?: RecordingOptions): Promise<RecordingSession>;
  stopRecording(sessionId: string): Promise<RecordingResult>;
}
```

with stable structures:

```ts
interface RecordingResult {
  path: string;
  durationMs: number;
  mimeType: string;
}
```

Now every implementation must convert platform-specific behavior into this common structure.

Therefore:

```text
macOS Swift ────┐
                │
Windows .NET ───┼──→ Audio Contract ──→ Chain SDK ──→ Mneme
                │
Future Linux ───┘
```

The contract is above every implementation.

---

# 7. Three Parts of a Good Contract

We should think of a contract as having three layers.

## Semantic Contract

Markdown explains what the API **means**.

Example:

```text
CONTRACT.md
```

It might explain:

> `Audio.startRecording()` starts microphone capture after required permission has been granted.
>
> If the user denies microphone permission, the operation returns the framework error `PERMISSION_DENIED`.
>
> The application must not receive platform-specific AVFoundation, WinRT, or .NET objects.

Markdown is useful because humans and AI can understand the intention.

---

## Structural Contract

TypeScript defines the exact API.

Example:

```ts
interface AudioApi {
  startRecording(options?: RecordingOptions): Promise<RecordingSession>;
  stopRecording(sessionId: string): Promise<RecordingResult>;
}
```

This makes the contract enforceable by the compiler.

---

## Behavioral Contract

Tests verify the implementation.

Example:

```ts
expect(session.id).toBeTruthy();
expect(result.durationMs).toBeGreaterThanOrEqual(0);
```

Together:

```text
CONTRACT.md
     │
     ▼
What should this mean?
     │
     ▼
contract.ts
     │
     ▼
What exact API exists?
     │
     ▼
contract tests
     │
     ▼
Does each native implementation obey it?
```

---

# 8. Do Not Port Existing Function Names Blindly

Suppose a Windows implementation already contains:

```csharp
StartAudioCaptureAsync()
```

We should NOT automatically expose:

```ts
startAudioCaptureAsync();
```

Likewise, if macOS uses a method named:

```swift
beginAVCaptureSession()
```

that should not dictate the public API.

Instead we ask:

> What concept does this represent for an application developer?

The better API may be:

```ts
desktop.audio.startRecording();
```

Native implementation names must not dictate the public SDK.

The contract should describe the shared concept.

---

# 9. Research Before Designing the API

Before creating a cross-platform capability, we research how each target platform handles it.

Example:

```text
Capability:
Audio Recording
```

Research:

```text
macOS
├── Which framework should be used?
├── How are permissions requested?
├── How are devices enumerated?
├── How are input changes reported?
├── What file formats are practical?
└── What happens during sleep/device removal?

Windows
├── Which API should be used?
├── How do microphone permissions behave?
├── How are devices enumerated?
├── Which APIs work with NativeAOT?
├── How are device changes reported?
└── What happens when a device disappears?

Future Linux
├── PipeWire
├── PulseAudio compatibility
├── Portal behavior
├── Desktop-environment differences
└── Permission limitations
```

Then we determine the common denominator.

Only after that should we finalize:

```ts
desktop.audio.startRecording();
```

Do not design the shared API by looking at only one operating system.

---

# 10. Research Must Become Permanent Knowledge

AI research is expensive.

If an agent spends thousands of tokens researching macOS audio behavior and then we throw away the conversation, we have wasted that work.

Research findings should therefore be written into the repository.

Example:

```text
capabilities/
└── audio/
    ├── AGENTS.md
    ├── CONTRACT.md
    ├── contract.ts
    ├── component.json
    │
    ├── research/
    │   ├── MACOS.md
    │   ├── WINDOWS.md
    │   ├── LINUX.md
    │   └── EDGE_CASES.md
    │
    ├── rust/
    │   └── AGENTS.md
    │
    ├── native/
    │   ├── macos/
    │   │   └── AGENTS.md
    │   └── windows/
    │       └── AGENTS.md
    │
    ├── js/
    └── tests/
```

Now future agents do not need to rediscover everything.

They read the existing research before touching implementation.

---

# 11. Repository as AI Memory

One of the most important principles of this project is:

> The repository should become the long-term memory of the AI agents.

Do not depend on conversation history.

Conversations disappear.

Agents change.

Models change.

Context windows have limits.

But repository documentation persists.

Therefore:

```text
Conversation
     │
     ▼
Research / decision
     │
     ▼
Repository documentation
     │
     ▼
Future AI reads it
```

Important decisions should move from chat into files.

The codebase must be maintainable by a completely fresh AI session.

---

# 12. Agent Memory Hierarchy

The repository should use layered memory.

At the root:

```text
AGENTS.md
```

This explains:

```text
Framework purpose
Core architecture
Rules that must never be broken
Public Chain SDK boundary
Runtime strategy
Native adapter strategy
Testing expectations
Context-loading rules
```

Each capability also gets:

```text
capabilities/audio/AGENTS.md
```

This explains only the audio capability.

Platform-specific implementations get smaller local memories:

```text
capabilities/audio/native/macos/AGENTS.md
capabilities/audio/native/windows/AGENTS.md
```

An agent working on Windows audio should not need to consume every file in the repository.

Its context can be:

```text
Root AGENTS.md
      +
Audio AGENTS.md
      +
Audio CONTRACT.md
      +
Audio WINDOWS.md research
      +
Windows adapter AGENTS.md
      +
Relevant source files
      +
Relevant tests
```

This creates a **context graph**, not one giant prompt.

---

# 13. Token Management

This project could consume enormous numbers of AI tokens if handled badly.

The worst approach is repeatedly asking:

> Understand the entire framework, Mneme, Windows, macOS, Rust, Swift, and .NET before implementing this small change.

Instead, give agents narrow context.

Example task:

```text
Implement the Windows Audio.startRecording() adapter.

Read:

/AGENTS.md
/docs/ARCHITECTURE.md
/capabilities/audio/AGENTS.md
/capabilities/audio/CONTRACT.md
/capabilities/audio/research/WINDOWS.md
/capabilities/audio/native/windows/AGENTS.md

Do not change the public contract.

Run Audio contract tests when complete.
```

Now the agent does not need the entire project.

---

# 14. Do Not Rediscover Existing Research

Important rule:

> If something has already been researched and documented, AI should read the existing research first.

Only perform new research when:

```text
Documentation is missing

OR

Documentation is outdated

OR

Implementation proves an assumption wrong

OR

Operating-system behavior changed

OR

The existing research does not cover the required edge case
```

This should save significant token usage.

---

# 15. Keep AI Tasks Small

Bad task:

> Build the entire cross-platform audio system.

Better:

```text
Task 1
Research macOS recording APIs.

Task 2
Research Windows recording APIs.

Task 3
Compare findings.

Task 4
Propose Audio contract.

Task 5
Review contract.

Task 6
Implement Rust coordination layer.

Task 7
Implement macOS Swift adapter.

Task 8
Implement Windows .NET adapter.

Task 9
Run shared contract tests.

Task 10
Record platform differences and edge cases.
```

Each agent receives only the context it needs.

---

# 16. Agents Should Produce Knowledge, Not Only Code

When an agent discovers something important, it should not leave that knowledge only inside its reasoning.

Example discovery:

> A Windows API behaves differently when a microphone is removed during capture.

That should become:

```text
capabilities/audio/research/WINDOWS.md
```

or:

```text
capabilities/audio/research/EDGE_CASES.md
```

The next agent should not need to discover it again.

The output of AI work should be:

```text
Code
+
Tests
+
Permanent knowledge
```

---

# 17. Capability Status

We should maintain a central file:

```text
CAPABILITY_MATRIX.md
```

Example:

| Capability      | macOS | Windows | Linux | Contract     |
| --------------- | ----- | ------- | ----- | ------------ |
| Platform/System | ✅    | 🧪      | ⏳    | Experimental |
| Files           | 🧪    | ⏳      | ⏳    | Draft        |
| Clipboard       | ⏳    | ⏳      | ⏳    | Research     |
| Audio Recording | ⏳    | ⏳      | ⏳    | Research     |
| Notifications   | ⏳    | ⏳      | ⏳    | Research     |
| Secure Storage  | ⏳    | ⏳      | ⏳    | Research     |
| Window          | ⏳    | ⏳      | ⏳    | Research     |

Legend:

```text
✅ Tested
🧪 Experimental
⏳ Not implemented
⚠ Partial
❌ Unsupported
```

This allows humans and agents to quickly understand project state.

---

# 18. Framework Candidate Tracking

Maintain:

```text
FRAMEWORK_CANDIDATES.md
```

Example:

```markdown
## Audio

Used by:

- Mneme

Generalizable:
Yes

Contract:
Research

macOS:
Not started

Windows:
Not started

Possible package:
@desktop/audio
```

Not everything Mneme uses automatically becomes framework functionality.

For example:

```text
Course parsing
Module organization
Study actions
AI learning prompts
Flashcard generation
```

belong to Mneme unless another application proves there is a useful generic abstraction.

---

# 19. When Something Becomes Framework-Worthy

Suggested progression:

```text
Needed by real application
        ↓
Research
        ↓
Draft contract
        ↓
Implement platform #1
        ↓
Implement platform #2
        ↓
Compare behavior
        ↓
Refine contract
        ↓
Real use in Mneme
        ↓
Framework candidate
```

A useful rule:

> Do not mark a contract stable based on only one operating system.

Otherwise we risk creating:

```text
macOS API
disguised as
cross-platform API
```

or:

```text
Windows API
disguised as
cross-platform API
```

The shared contract must be based on shared meaning.

---

# 20. Capability Detection Instead of Platform Detection

Applications should generally avoid:

```ts
if (platform === "windows") {
}
```

Prefer:

```ts
if (desktop.capabilities.audioRecording) {
}
```

Why?

Because operating system does not always fully determine capability.

A machine may lack:

```text
Microphone permission
Specific hardware
Required OS version
Required service
Specific native feature
```

Future Linux support will make this even more important because capability can depend on:

```text
Distribution
Desktop environment
Display server
Compositor
Installed services
Portal availability
```

Therefore ask:

> Can the system do this?

instead of:

> Which operating system is this?

Platform information is still useful for diagnostics and explicitly platform-specific UX, but capability checks should drive behavior.

---

# 21. Edge Cases Are Part of the Contract

We should research edge cases before declaring APIs stable.

For `Audio`:

```text
Permission denied
No microphone
Microphone removed
Default microphone changes
Bluetooth headset disconnects
Application sleeps
System sleeps
Recording is interrupted
Disk becomes unavailable
Very long recording
Device sample rate changes
User cancels recording
```

For `Files`:

```text
User cancels picker
File is deleted after selection
Permission changes
Network drive disappears
Cloud file is not locally available
Path contains Unicode
File is locked
File is extremely large
Symlink behavior
```

For `Clipboard`:

```text
Clipboard is empty
Non-text content
Large content
Permission/privacy restrictions
Clipboard changes during operation
```

The API should define predictable behavior.

---

# 22. Common Error Model

Native platforms produce completely different errors.

Applications should not need to understand all of them.

We should normalize errors.

Example:

```ts
type ChainErrorCode =
  | "UNSUPPORTED"
  | "PERMISSION_DENIED"
  | "NOT_FOUND"
  | "UNAVAILABLE"
  | "INVALID_ARGUMENT"
  | "CANCELLED"
  | "NATIVE_FAILURE";
```

Native details can still be available for debugging.

Example:

```ts
interface ChainError {
  code: ChainErrorCode;
  message: string;
  nativeCode?: string;
  debugDetails?: unknown;
}
```

Application logic should normally depend on `code`, not platform-specific errors.

---

# 23. Events Instead of Constant Polling

Whenever the operating system can notify us, use events.

Prefer:

```ts
desktop.audio.onDeviceChanged(...);
desktop.files.onChanged(...);
desktop.power.onSleep(...);
desktop.clipboard.onChanged(...);
```

instead of:

```ts
setInterval(checkSomething, 1000);
```

This reduces:

```text
CPU usage
Battery consumption
Bridge traffic
Unnecessary wakeups
Duplicated state checks
```

This matters especially for lightweight desktop applications.

---

# 24. Performance and Responsibility Boundaries

Using React does not mean everything should move into JavaScript.

JavaScript/TypeScript should handle:

```text
UI
Rich text editor
Course/module presentation
Application state
User interactions
AI workflow orchestration
Custom prompts
Study tools
Configuration
```

Rust should handle:

```text
Cross-platform coordination
IPC
Serialization
Permission enforcement
Module lifecycle
Common native logic
Native adapter routing
Stable error normalization
```

Platform-native modules should handle:

```text
OS integration
Microphone/audio capture where native APIs are best
Secure storage
Platform-specific window behavior
System events
Performance-critical native operations
Deep platform integrations
```

Therefore:

```text
React
   ↓
asks framework to perform capability
   ↓
Rust coordinates
   ↓
Native implementation performs OS work
   ↓
Rust normalizes result/event
   ↓
React receives stable data
```

---

# 25. Mneme's Rich Editor and AI Are Application Features

Mneme contains functionality that should **not automatically move into the desktop framework**.

Examples:

```text
Notion-style editor
Course hierarchy
Module importer
Study summaries
Flashcards
Quizzes
Custom AI actions
AI context profiles
Prepare Module workflow
Agent study behavior
```

These are Mneme product features.

They may use framework capabilities:

```text
Files
Audio
Clipboard
Secure Storage
Notifications
Local Processes
```

but they do not belong in the framework simply because Mneme uses them.

For example:

```text
Mneme Agent
    ↓
needs recording
    ↓
desktop.audio
```

The framework knows how to record audio.

It does not know what a "lecture" or "module summary" means.

This separation should remain clear.

---

# 26. Native Module Escape Hatch

The Chain SDK should never trap developers.

If the framework does not expose something:

```text
Developer
    ↓
Custom Native Module
    ↓
Rust / Swift / .NET / platform implementation
```

Possible future workflow:

```bash
desktop module create my-feature
```

Then:

```ts
import MyFeature from "./native/my-feature";
```

This is similar in philosophy to native modules in other cross-platform ecosystems.

The framework should make common capabilities easy without making uncommon capabilities impossible.

---

# 27. We Do Not Need Every Native API

We only want to standardize concepts that make sense across desktop platforms.

Examples:

```text
Files
File Dialogs
Display
Camera
Microphone
Audio
Media
Notifications
Clipboard
Power
System
Window
Tray
Shortcuts
Storage
Secure Storage
Process
Deep Links
Screen Capture
```

Extremely platform-specific functionality should remain custom native modules.

Do not force every API into a fake cross-platform abstraction.

---

# 28. Native Implementation Strategy

Not every capability must be implemented entirely in Rust.

Use the best layer for the job.

Decision order:

```text
Can Rust implement the feature cleanly and portably?
        │
       YES
        ▼
      Rust
```

If not:

```text
Does the OS expose a significantly better native API?
        │
       YES
        ▼
Use platform-native adapter
```

Preferred platform implementations:

```text
macOS
├── Swift
├── Objective-C where bridging is required
├── AppKit
├── Foundation
├── AVFoundation
└── other Apple frameworks

Windows
├── C#
├── .NET
├── NativeAOT where appropriate
├── WinRT
├── Win32
└── Windows App SDK where appropriate
```

Rust remains the stable coordinator.

---

# 29. Rust ↔ Native Bridge Rules

Native language boundaries must be deliberate.

## macOS

Preferred shape:

```text
Rust
  ↓
Stable C ABI / Objective-C bridge
  ↓
Swift
  ↓
Apple frameworks
```

Do not expose Swift objects across the Rust boundary.

Use simple data:

```text
Strings
Numbers
Booleans
Byte buffers
Stable structs/serialized data
Opaque handles with explicit lifecycle
```

---

## Windows

Preferred shape:

```text
Rust
  ↓
Native FFI
  ↓
.NET NativeAOT shared library
  ↓
C#
  ↓
Windows APIs
```

Do not expose managed .NET objects across FFI.

Use stable entry points and explicit ownership.

The public TypeScript API must remain unaware of any of this.

---

# 30. AI-Native Architecture

One defining characteristic of this project is that it is designed to be maintained heavily with AI.

We should take advantage of that deliberately.

Each capability should be small and self-contained:

```text
capabilities/
└── audio/
    ├── AGENTS.md
    ├── CONTRACT.md
    ├── contract.ts
    ├── component.json
    │
    ├── research/
    │   ├── MACOS.md
    │   ├── WINDOWS.md
    │   ├── LINUX.md
    │   └── EDGE_CASES.md
    │
    ├── rust/
    │   ├── AGENTS.md
    │   └── src/
    │
    ├── native/
    │   ├── macos/
    │   │   ├── AGENTS.md
    │   │   └── ...
    │   └── windows/
    │       ├── AGENTS.md
    │       └── ...
    │
    ├── js/
    │   └── ...
    │
    └── tests/
```

An AI agent can be told:

> Implement the Windows Audio adapter. Do not modify the public contract.

This is much safer than:

> Understand the entire desktop framework and implement Windows audio.

---

# 31. AI-Assisted Native Porting

The framework should explicitly support AI-assisted porting between native implementations.

Example:

```text
Existing macOS Swift implementation
            +
Shared CONTRACT.md
            +
Rust bridge contract
            +
Windows research
            ↓
AI Agent
            ↓
Equivalent Windows C# implementation
```

The existing implementation is a **behavioral reference**.

It is not the contract.

The contract remains the source of truth.

Likewise:

```text
Existing Windows .NET implementation
            +
Shared CONTRACT.md
            +
macOS research
            ↓
AI Agent
            ↓
Equivalent Swift implementation
```

The agent must not blindly translate source code.

It should reproduce the same externally observable behavior using the correct native API for that platform.

---

# 32. Human and AI Responsibilities

## Human

The human should primarily decide:

```text
What problem are we solving?

Does the API feel good?

Is this abstraction useful?

Should this belong in the framework?

Does the naming make sense?

Does Mneme actually need this?

Are we making something unnecessarily complicated?

Is the AI-generated architecture understandable?
```

The human does not need to personally know every native API.

---

## AI

AI can help with:

```text
Native API research
Platform comparison
Swift implementation
.NET implementation
Rust implementation
Chain SDK implementation
Tests
Documentation
Edge-case discovery
Compatibility analysis
Refactoring
Migration
Porting
Code review
```

But important architectural decisions must remain visible and understandable to the human.

---

# 33. Never Let AI Complexity Hide the Architecture

If AI generates something we cannot reasonably explain at a high level, stop.

We should always understand:

```text
Mneme / App
      ↓
Chain SDK
      ↓
Contract
      ↓
Chain Core (Rust)
      ↓
Native Adapter
      ↓
Operating System
```

We do not necessarily need to understand every native implementation detail.

But we must understand why each layer exists.

Tauri must also remain an implementation detail:

```text
App
 ↓
Chain SDK
 ↓
Chain Core (Rust)
 ↓
Tauri initially
 ↓
Native/runtime
```

Never:

```text
Mneme
 ↓
Tauri directly
```

---

# 34. Tauri Is the Initial Runtime, Not the Public Framework

The first runtime implementation uses Tauri because it already solves difficult infrastructure problems such as:

```text
WebView lifecycle
IPC plumbing
Packaging
Permissions/capabilities
Window integration
Plugin infrastructure
Build tooling
```

We should use that work instead of rebuilding everything immediately.

However:

> Applications must not depend directly on Tauri.

Mneme should never require:

```ts
import { invoke } from "@tauri-apps/api/core";
```

Mneme should use:

```ts
import { desktop } from "@chain/sdk";

const info = await desktop.platform.getInfo();
```

This gives us an escape path.

Future architecture could become:

```text
Stage 1

Chain SDK
    ↓
Chain Core (Rust)
    ↓
Tauri
    ↓
Wry / native runtime
```

Later:

```text
Stage 2

Chain SDK
    ↓
Chain Core (Rust)
    ↓
Our Runtime
    ↓
Wry / TAO / native platform
```

If Tauri is eventually replaced, Mneme should not need to change.

---

# 35. Recommended Repository Separation

Mneme and the reusable framework should be conceptually separate.

Recommended direction:

```text
mneme/
```

contains the product.

And:

```text
chain/
```

contains Chain's reusable runtime, contracts, research, native adapters, and SDK code.

Possible framework structure:

```text
chain/
│
├── AGENTS.md
│
├── docs/
│   ├── ARCHITECTURE.md
│   ├── API_NAMING_RULES.md
│   ├── CAPABILITY_MATRIX.md
│   ├── FRAMEWORK_CANDIDATES.md
│   └── adr/
│
├── context/
│   └── registry.json
│
├── capabilities/
│   ├── platform/
│   ├── files/
│   ├── audio/
│   ├── clipboard/
│   └── notifications/
│
├── packages/
│   └── sdk/
│
├── crates/
│   └── core/
│
├── apps/
│   └── playground/
│
└── tests/
```

Mneme consumes the Chain SDK.

---

# 36. Do Not Generalize Too Early

Even if Mneme and the framework use separate repositories, we must not generalize every Mneme feature into the framework.

The correct progression remains:

```text
Mneme need
   ↓
Capability experiment
   ↓
Research
   ↓
Contract
   ↓
Native implementation
   ↓
Real use
   ↓
Does abstraction survive?
   ↓
Reusable capability
```

Do not create unnecessary framework packages before boundaries are clear.

Repository separation does not mean conceptual over-generalization.

---

# 37. Our Development Loop

The ideal loop is:

```text
I have a Mneme requirement
        ↓
Discuss with AI
        ↓
Is this app logic or desktop capability?
        ↓
If capability:
        ↓
Check existing repository knowledge
        ↓
Research missing platform behavior
        ↓
Review findings
        ↓
Design/update contract
        ↓
Implement Rust/native adapters
        ↓
Expose through Chain SDK
        ↓
Use in Mneme
        ↓
Test in real workflow
        ↓
Discover problems
        ↓
Improve contract
        ↓
Document findings
        ↓
Knowledge stays in repository
```

Then repeat.

---

# 38. Why Documentation Matters So Much

Normally documentation is something developers write after implementation.

For this project, documentation is part of the infrastructure.

It serves:

```text
Humans
+
AI agents
+
Future contributors
+
Future ourselves
```

Good documentation reduces:

```text
Repeated research
Repeated prompts
Token consumption
Wrong assumptions
Architectural drift
Agent hallucination
Unsafe native changes
Cross-platform inconsistencies
```

Therefore Markdown is not just documentation.

It is part of the project's **knowledge system**.

---

# 39. Token Rule

Before asking AI to research something:

> Check whether we already know it.

Before asking AI to understand the whole project:

> Determine the smallest set of files it actually needs.

Before giving an agent a massive task:

> Break it into bounded tasks.

Before ending useful research:

> Save the findings into the repository.

The objective is:

```text
AI tokens
   ↓
produce
   ↓
Permanent knowledge
```

rather than:

```text
AI tokens
   ↓
temporary conversation
   ↓
forgotten
   ↓
pay tokens again
```

---

# 40. What We Should Avoid

Avoid:

```text
Building 50 modules immediately

Making Mneme import Tauri directly

Researching the same platform repeatedly

Letting every agent redesign contracts

Porting Swift or C# method names directly to TypeScript

Blindly translating Swift implementations into C#

Blindly translating C# implementations into Swift

Making React responsible for native system work

Pretending macOS and Windows behave identically

Forcing every feature into Rust

Forcing every feature into Swift or .NET

Huge prompts containing the entire repository

Undocumented architectural decisions

AI-generated complexity nobody understands

Moving Mneme-specific product logic into the framework

Treating one platform implementation as the source of truth
```

---

# 41. What We Want

We want:

```text
Real Mneme requirement
        ↓
Small capability
        ↓
Good research
        ↓
Simple contract
        ↓
Rust/native implementation
        ↓
Real-world test
        ↓
Documented knowledge
        ↓
Reusable module
```

Repeat that enough times and eventually:

```text
Mneme
Wallpache
Lazify
Other Apps
    │
    ▼
Chain SDK
    │
    ▼
macOS + Windows + future Linux
```

---

# 42. The Bigger Idea

We may eventually end up with something resembling:

> An Expo-style development experience for desktop applications.

Potentially:

```bash
chain create my-app

chain add files
chain add audio
chain add clipboard
chain add tray

chain dev

chain build --all
```

Developers would write:

```ts
import { desktop } from "@chain/sdk";

const file = await desktop.files.pick();
const info = await desktop.platform.getInfo();
```

while the framework decides how each capability is implemented underneath.

But that is **not today's objective**.

Today's objective is:

> Build the minimal framework foundation correctly, then build Mneme through it and allow real application requirements to shape the framework.

If the framework proves useful across Mneme and other applications, we can decide how far to take it.

---

# 43. Initial Framework Milestone

The first implementation should be intentionally small.

Implement only:

```ts
desktop.platform.getInfo();
```

Expected architecture:

```text
Playground / Mneme test
        ↓
Chain SDK
        ↓
Chain Core (Rust)
        ↓
Tauri runtime
        ↓
Operating System
        ↓
Chain Core (Rust)
        ↓
Chain SDK
        ↓
Application
```

Expected response:

```ts
{
  os: "macos" | "windows",
  arch: "arm64" | "x64",
  runtimeVersion: string
}
```

Before coding, create:

```text
AGENTS.md
docs/ARCHITECTURE.md
capabilities/platform/AGENTS.md
capabilities/platform/CONTRACT.md
capabilities/platform/contract.ts
capabilities/platform/component.json
```

Once this vertical slice works, freeze the pattern.

Then Mneme can drive the next capability.

---

# 44. Likely Mneme Capability Order

Do not treat this as a promise.

This is only the likely order based on current Mneme requirements.

```text
1. Platform/System
2. Files + File Dialogs
3. Clipboard
4. Secure Storage
5. Window
6. Notifications
7. Audio Recording
8. Audio Playback
9. Deep Links
10. File Watching
11. Local Process / Model Runtime
12. Screen Capture or Accessibility only if Mneme genuinely needs them
```

Each capability must earn its place through a real requirement.

---

# 45. Final Mental Model

Whenever we work on Mneme, think:

```text
                    MNEME NEED
                         │
                         ▼
            App feature or native capability?
                  │                 │
               App logic         Capability
                  │                 │
                  ▼                 ▼
                Mneme       Check existing research
                                    │
                                    ▼
                            Research missing pieces
                                    │
                                    ▼
                              Common meaning
                                    │
                                    ▼
                            TypeScript contract
                                    │
                                    ▼
                               Chain Core (Rust)
                                    │
                         ┌──────────┴──────────┐
                         ▼                     ▼
                      macOS                 Windows
                   Swift / Obj-C          C# / .NET
                         │                     │
                         └──────────┬──────────┘
                                    ▼
                               Real testing
                                    │
                                    ▼
                              Record findings
                                    │
                                    ▼
                           Framework capability?
                              │             │
                             YES            NO
                              │             │
                              ▼             ▼
                         Chain SDK      Mneme
                           module          specific
```

---

# 46. One Rule to Remember

If everything else in this document becomes confusing, remember this:

> **We are not trying to predict the perfect desktop framework. We are building the smallest sound framework foundation, then using Mneme as the first real application to discover what cross-platform desktop development actually needs. The parts that prove reusable become framework capabilities.**

And every time AI helps us learn something expensive:

> **Write that knowledge into the repository so the next AI agent does not have to learn it again.**

The contract remains the source of truth.

The repository remains the long-term memory.

Mneme remains the first real experiment.
