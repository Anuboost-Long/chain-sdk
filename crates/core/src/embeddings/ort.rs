//! The ONNX Runtime C API calls embeddings needs, on the runtime sherpa-onnx
//! already links statically (1.28.2 in sherpa-onnx 1.13.8), so there's no
//! second copy and nothing to download. ONNX Runtime publishes its API as a
//! table of function pointers, `struct OrtApi` in onnxruntime_c_api.h; the
//! indices below are positions in it. The table only ever grows at the
//! end, and every entry used here is from API version 1, so they hold for
//! any runtime sherpa-onnx moves to.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::ptr::null_mut;
use std::sync::OnceLock;

type Status = *mut c_void;
type Ptr = *mut c_void;

#[repr(C)]
struct OrtApiBase {
    get_api: unsafe extern "system" fn(version: u32) -> *const *const c_void,
    get_version_string: unsafe extern "system" fn() -> *const c_char,
}

extern "system" {
    fn OrtGetApiBase() -> *const OrtApiBase;
}

// OrtErrorCode values.
const NO_SUCHFILE: i32 = 3;
const INVALID_PROTOBUF: i32 = 7;
const INVALID_GRAPH: i32 = 10;

pub const ELEMENT_FLOAT: i32 = 1;
pub const ELEMENT_INT32: i32 = 6;
pub const ELEMENT_INT64: i32 = 7;

const LOGGING_LEVEL_ERROR: i32 = 3;
const GRAPH_OPTIMIZATION_ALL: i32 = 99;
const ARENA_ALLOCATOR: i32 = 1;
const MEM_TYPE_DEFAULT: i32 = 0;

#[derive(Debug)]
pub enum OrtError {
    /// The file isn't there.
    NotFound(String),
    /// Not an ONNX model ONNX Runtime can load.
    InvalidModel(String),
    OutOfMemory(String),
    Other(String),
}

#[cfg(windows)]
type PathChar = u16;
#[cfg(not(windows))]
type PathChar = c_char;

struct Api {
    get_error_code: unsafe extern "system" fn(Status) -> i32,
    get_error_message: unsafe extern "system" fn(Status) -> *const c_char,
    create_env: unsafe extern "system" fn(i32, *const c_char, *mut Ptr) -> Status,
    create_session: unsafe extern "system" fn(Ptr, *const PathChar, Ptr, *mut Ptr) -> Status,
    run: unsafe extern "system" fn(Ptr, Ptr, *const *const c_char, *const Ptr, usize, *const *const c_char, usize, *mut Ptr) -> Status,
    create_session_options: unsafe extern "system" fn(*mut Ptr) -> Status,
    set_graph_optimization_level: unsafe extern "system" fn(Ptr, i32) -> Status,
    set_intra_op_threads: unsafe extern "system" fn(Ptr, i32) -> Status,
    input_count: unsafe extern "system" fn(Ptr, *mut usize) -> Status,
    output_count: unsafe extern "system" fn(Ptr, *mut usize) -> Status,
    input_type_info: unsafe extern "system" fn(Ptr, usize, *mut Ptr) -> Status,
    output_type_info: unsafe extern "system" fn(Ptr, usize, *mut Ptr) -> Status,
    input_name: unsafe extern "system" fn(Ptr, usize, Ptr, *mut *mut c_char) -> Status,
    output_name: unsafe extern "system" fn(Ptr, usize, Ptr, *mut *mut c_char) -> Status,
    create_run_options: unsafe extern "system" fn(*mut Ptr) -> Status,
    run_options_set_terminate: unsafe extern "system" fn(Ptr) -> Status,
    create_tensor: unsafe extern "system" fn(Ptr, Ptr, usize, *const i64, usize, i32, *mut Ptr) -> Status,
    tensor_data: unsafe extern "system" fn(Ptr, *mut Ptr) -> Status,
    cast_to_tensor_info: unsafe extern "system" fn(Ptr, *mut Ptr) -> Status,
    element_type: unsafe extern "system" fn(Ptr, *mut i32) -> Status,
    dimensions_count: unsafe extern "system" fn(Ptr, *mut usize) -> Status,
    dimensions: unsafe extern "system" fn(Ptr, *mut i64, usize) -> Status,
    tensor_type_and_shape: unsafe extern "system" fn(Ptr, *mut Ptr) -> Status,
    create_cpu_memory_info: unsafe extern "system" fn(i32, i32, *mut Ptr) -> Status,
    allocator_free: unsafe extern "system" fn(Ptr, Ptr) -> Status,
    default_allocator: unsafe extern "system" fn(*mut Ptr) -> Status,
    release_status: unsafe extern "system" fn(Status),
    release_memory_info: unsafe extern "system" fn(Ptr),
    release_session: unsafe extern "system" fn(Ptr),
    release_value: unsafe extern "system" fn(Ptr),
    release_run_options: unsafe extern "system" fn(Ptr),
    release_type_info: unsafe extern "system" fn(Ptr),
    release_tensor_type_and_shape: unsafe extern "system" fn(Ptr),
    release_session_options: unsafe extern "system" fn(Ptr),
}

fn api() -> &'static Api {
    static API: OnceLock<Api> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let table = ((*OrtGetApiBase()).get_api)(1);
        assert!(!table.is_null(), "the linked ONNX Runtime has no API version 1");
        macro_rules! at {
            ($index:expr) => {
                std::mem::transmute::<*const c_void, _>(*table.add($index))
            };
        }
        Api {
            get_error_code: at!(1),
            get_error_message: at!(2),
            create_env: at!(3),
            create_session: at!(7),
            run: at!(9),
            create_session_options: at!(10),
            set_graph_optimization_level: at!(23),
            set_intra_op_threads: at!(24),
            input_count: at!(30),
            output_count: at!(31),
            input_type_info: at!(33),
            output_type_info: at!(34),
            input_name: at!(36),
            output_name: at!(37),
            create_run_options: at!(39),
            run_options_set_terminate: at!(46),
            create_tensor: at!(49),
            tensor_data: at!(51),
            cast_to_tensor_info: at!(55),
            element_type: at!(60),
            dimensions_count: at!(61),
            dimensions: at!(62),
            tensor_type_and_shape: at!(65),
            create_cpu_memory_info: at!(69),
            allocator_free: at!(76),
            default_allocator: at!(78),
            release_status: at!(93),
            release_memory_info: at!(94),
            release_session: at!(95),
            release_value: at!(96),
            release_run_options: at!(97),
            release_type_info: at!(98),
            release_tensor_type_and_shape: at!(99),
            release_session_options: at!(100),
        }
    })
}

#[cfg(test)]
pub fn version() -> String {
    unsafe { CStr::from_ptr(((*OrtGetApiBase()).get_version_string)()) }.to_string_lossy().into_owned()
}

fn check(status: Status) -> Result<(), OrtError> {
    if status.is_null() {
        return Ok(());
    }
    let api = api();
    let (code, message) = unsafe {
        let message = CStr::from_ptr((api.get_error_message)(status)).to_string_lossy().into_owned();
        let code = (api.get_error_code)(status);
        (api.release_status)(status);
        (code, message)
    };
    Err(match code {
        NO_SUCHFILE => OrtError::NotFound(message),
        INVALID_PROTOBUF | INVALID_GRAPH => OrtError::InvalidModel(message),
        _ if message.contains("bad_alloc") || message.contains("Failed to allocate") => OrtError::OutOfMemory(message),
        _ => OrtError::Other(message),
    })
}

/// One for the process, never released: ONNX Runtime shares a single
/// environment with sherpa-onnx's sessions anyway.
fn env() -> Result<Ptr, OrtError> {
    static ENV: OnceLock<usize> = OnceLock::new();
    if let Some(&env) = ENV.get() {
        return Ok(env as Ptr);
    }
    let mut env = null_mut();
    check(unsafe { (api().create_env)(LOGGING_LEVEL_ERROR, c"chain-embeddings".as_ptr(), &mut env) })?;
    Ok(*ENV.get_or_init(|| env as usize) as Ptr)
}

#[cfg(windows)]
fn path_chars(path: &Path) -> Vec<PathChar> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain([0]).collect()
}

#[cfg(not(windows))]
fn path_chars(path: &Path) -> Vec<PathChar> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).unwrap_or_default().into_bytes_with_nul().into_iter().map(|b| b as c_char).collect()
}

pub struct TensorInfo {
    pub name: CString,
    pub element_type: i32,
    /// -1 for a dimension fixed only at run time.
    pub shape: Vec<i64>,
}

pub struct Session(Ptr);

// ONNX Runtime sessions may be run from any thread, concurrently.
unsafe impl Send for Session {}
unsafe impl Sync for Session {}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { (api().release_session)(self.0) }
    }
}

impl Session {
    pub fn open(model: &Path, threads: i32) -> Result<Session, OrtError> {
        let api = api();
        let env = env()?;
        let mut options = null_mut();
        check(unsafe { (api.create_session_options)(&mut options) })?;
        let session = (|| {
            check(unsafe { (api.set_intra_op_threads)(options, threads) })?;
            check(unsafe { (api.set_graph_optimization_level)(options, GRAPH_OPTIMIZATION_ALL) })?;
            let mut session = null_mut();
            check(unsafe { (api.create_session)(env, path_chars(model).as_ptr(), options, &mut session) })?;
            Ok(Session(session))
        })();
        unsafe { (api.release_session_options)(options) };
        session
    }

    pub fn inputs(&self) -> Result<Vec<TensorInfo>, OrtError> {
        let api = api();
        self.describe(api.input_count, api.input_name, api.input_type_info)
    }

    pub fn outputs(&self) -> Result<Vec<TensorInfo>, OrtError> {
        let api = api();
        self.describe(api.output_count, api.output_name, api.output_type_info)
    }

    fn describe(
        &self,
        count_of: unsafe extern "system" fn(Ptr, *mut usize) -> Status,
        name_of: unsafe extern "system" fn(Ptr, usize, Ptr, *mut *mut c_char) -> Status,
        type_of: unsafe extern "system" fn(Ptr, usize, *mut Ptr) -> Status,
    ) -> Result<Vec<TensorInfo>, OrtError> {
        let api = api();
        let mut allocator = null_mut();
        check(unsafe { (api.default_allocator)(&mut allocator) })?;
        let mut count = 0;
        check(unsafe { count_of(self.0, &mut count) })?;
        (0..count)
            .map(|index| unsafe {
                let mut raw = null_mut();
                check(name_of(self.0, index, allocator, &mut raw))?;
                let name = CStr::from_ptr(raw).to_owned();
                check((api.allocator_free)(allocator, raw.cast()))?;
                let mut type_info = null_mut();
                check(type_of(self.0, index, &mut type_info))?;
                // A non-tensor input (a sequence, a map) has no tensor info.
                let mut tensor = null_mut();
                let shaped = check((api.cast_to_tensor_info)(type_info, &mut tensor))
                    .and_then(|_| if tensor.is_null() { Ok((0, Vec::new())) } else { element_and_shape(tensor) });
                (api.release_type_info)(type_info);
                let (element_type, shape) = shaped?;
                Ok(TensorInfo { name, element_type, shape })
            })
            .collect()
    }

    /// Runs the model on `inputs` and returns the one output named
    /// `output`, as float32 with its shape.
    pub fn run(&self, inputs: &mut [Input], output: &CStr, options: &RunOptions) -> Result<(Vec<i64>, Vec<f32>), OrtError> {
        let api = api();
        let mut memory = null_mut();
        check(unsafe { (api.create_cpu_memory_info)(ARENA_ALLOCATOR, MEM_TYPE_DEFAULT, &mut memory) })?;
        let mut values: Vec<Ptr> = Vec::with_capacity(inputs.len());
        let result = (|| {
            for input in inputs.iter_mut() {
                let (data, bytes, element_type) = match &mut input.data {
                    InputData::Int64(v) => (v.as_mut_ptr().cast(), v.len() * 8, ELEMENT_INT64),
                    InputData::Int32(v) => (v.as_mut_ptr().cast(), v.len() * 4, ELEMENT_INT32),
                };
                let mut value = null_mut();
                check(unsafe {
                    (api.create_tensor)(memory, data, bytes, input.shape.as_ptr(), input.shape.len(), element_type, &mut value)
                })?;
                values.push(value);
            }
            let names: Vec<*const c_char> = inputs.iter().map(|input| input.name.as_ptr()).collect();
            let mut out = null_mut();
            check(unsafe {
                (api.run)(self.0, options.0, names.as_ptr(), values.as_ptr(), values.len(), &output.as_ptr(), 1, &mut out)
            })?;
            let read = unsafe { read_float_tensor(out) };
            unsafe { (api.release_value)(out) };
            read
        })();
        for value in values {
            unsafe { (api.release_value)(value) };
        }
        unsafe { (api.release_memory_info)(memory) };
        result
    }
}

unsafe fn element_and_shape(tensor_info: Ptr) -> Result<(i32, Vec<i64>), OrtError> {
    let api = api();
    let mut element_type = 0;
    check((api.element_type)(tensor_info, &mut element_type))?;
    let mut rank = 0;
    check((api.dimensions_count)(tensor_info, &mut rank))?;
    let mut shape = vec![0i64; rank];
    check((api.dimensions)(tensor_info, shape.as_mut_ptr(), rank))?;
    Ok((element_type, shape))
}

unsafe fn read_float_tensor(value: Ptr) -> Result<(Vec<i64>, Vec<f32>), OrtError> {
    let api = api();
    let mut info = null_mut();
    check((api.tensor_type_and_shape)(value, &mut info))?;
    let shaped = element_and_shape(info);
    (api.release_tensor_type_and_shape)(info);
    let (element_type, shape) = shaped?;
    if element_type != ELEMENT_FLOAT {
        return Err(OrtError::InvalidModel(format!("the output is element type {element_type}, not float32")));
    }
    let len = shape.iter().product::<i64>().max(0) as usize;
    let mut data = null_mut();
    check((api.tensor_data)(value, &mut data))?;
    Ok((shape, std::slice::from_raw_parts(data as *const f32, len).to_vec()))
}

pub enum InputData {
    Int64(Vec<i64>),
    Int32(Vec<i32>),
}

pub struct Input {
    pub name: CString,
    pub shape: Vec<i64>,
    pub data: InputData,
}

/// Lets another thread stop a run in progress.
pub struct RunOptions(Ptr);

// RunOptionsSetTerminate exists to be called from another thread.
unsafe impl Send for RunOptions {}
unsafe impl Sync for RunOptions {}

impl Drop for RunOptions {
    fn drop(&mut self) {
        unsafe { (api().release_run_options)(self.0) }
    }
}

impl RunOptions {
    pub fn new() -> Result<RunOptions, OrtError> {
        let mut options = null_mut();
        check(unsafe { (api().create_run_options)(&mut options) })?;
        Ok(RunOptions(options))
    }

    /// The run in progress, and every later one with these options, fails.
    pub fn terminate(&self) {
        let _ = check(unsafe { (api().run_options_set_terminate)(self.0) });
    }
}
