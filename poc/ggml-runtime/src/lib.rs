use anyhow::{anyhow, bail, Context as _, Result};
use ggml_sys as sys;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::io::{Read, Seek, SeekFrom};
use std::os::raw::c_void;
use std::path::Path;
use std::ptr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendKind {
    Cpu,
    Metal,
}

pub struct Backend {
    raw: sys::ggml_backend_t,
    pub kind: BackendKind,
}

impl Backend {
    pub fn new(kind: BackendKind) -> Result<Self> {
        if kind == BackendKind::Cpu {
            let raw = unsafe { sys::ggml_backend_cpu_init() };
            if raw.is_null() {
                bail!("failed to initialize CPU backend");
            }
            return Ok(Self { raw, kind });
        }
        if !cfg!(target_os = "macos") {
            bail!("Metal is only supported on macOS");
        }
        unsafe { sys::ggml_backend_load_all() };
        let device = unsafe { sys::ggml_backend_dev_by_type(sys::GGML_BACKEND_DEVICE_TYPE_GPU) };
        if device.is_null() {
            bail!("Metal backend is unavailable");
        }
        let raw = unsafe { sys::ggml_backend_dev_init(device, ptr::null()) };
        if raw.is_null() {
            bail!("failed to initialize Metal backend");
        }
        Ok(Self { raw, kind })
    }

    pub fn raw(&self) -> sys::ggml_backend_t {
        self.raw
    }

    pub fn set_threads(&self, threads: usize) {
        if self.kind == BackendKind::Cpu {
            unsafe { sys::ggml_backend_cpu_set_n_threads(self.raw, threads.max(1) as i32) };
        }
    }

    pub fn synchronize(&self) {
        unsafe { sys::ggml_backend_synchronize(self.raw) };
    }
}

impl BackendKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Metal => "Metal",
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        unsafe { sys::ggml_backend_free(self.raw) };
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Shape {
    dims: [i64; 4],
    rank: usize,
}

impl Shape {
    pub fn new(dims: &[i64]) -> Result<Self> {
        if dims.is_empty() || dims.len() > 4 || dims.iter().any(|dimension| *dimension <= 0) {
            bail!("invalid tensor shape {dims:?}");
        }
        dims.iter().try_fold(1usize, |elements, dimension| {
            let dimension =
                usize::try_from(*dimension).context("tensor dimension exceeds usize")?;
            elements
                .checked_mul(dimension)
                .context("tensor element count overflows usize")
        })?;
        let mut stored = [1; 4];
        stored[..dims.len()].copy_from_slice(dims);
        Ok(Self {
            dims: stored,
            rank: dims.len(),
        })
    }

    pub fn to_vec(self) -> Vec<i64> {
        self.dims[..self.rank].to_vec()
    }

    pub fn rank(self) -> usize {
        self.rank
    }

    pub fn at(self, axis: usize) -> i64 {
        self.dims[axis]
    }

    pub fn last(self) -> i64 {
        self.dims[self.rank - 1]
    }

    pub fn elements(self) -> usize {
        self.dims[..self.rank]
            .iter()
            .map(|dimension| usize::try_from(*dimension).expect("validated tensor dimension"))
            .try_fold(1usize, usize::checked_mul)
            .expect("validated tensor element count")
    }

    fn ggml_dims(self) -> [i64; 4] {
        let mut dims = [1; 4];
        for index in 0..self.rank {
            dims[index] = self.dims[self.rank - 1 - index];
        }
        dims
    }

    fn with_axis(self, axis: usize, value: i64) -> Self {
        let mut dimensions = self.to_vec();
        dimensions[axis] = value;
        Self::new(&dimensions).expect("derived tensor shape must remain valid")
    }
}

#[derive(Clone, Copy)]
pub struct Tensor {
    pub raw: *mut sys::ggml_tensor,
    pub shape: Shape,
    pub kind: sys::ggml_type,
}

impl Tensor {
    fn new(raw: *mut sys::ggml_tensor, shape: Shape, kind: sys::ggml_type) -> Result<Self> {
        if raw.is_null() {
            bail!("GGML returned a null tensor for shape {:?}", shape.to_vec());
        }
        let expected = shape.ggml_dims();
        for (index, dimension) in expected.iter().enumerate().take(shape.rank) {
            if unsafe { (*raw).ne[index] } != *dimension {
                bail!(
                    "GGML tensor shape differs from logical shape {:?}",
                    shape.to_vec()
                );
            }
        }
        Ok(Self { raw, shape, kind })
    }
}

struct GgufContext {
    raw: *mut sys::gguf_context,
}

impl Drop for GgufContext {
    fn drop(&mut self) {
        unsafe { sys::gguf_free(self.raw) };
    }
}

struct Context {
    raw: *mut sys::ggml_context,
}

impl Context {
    fn new(bytes: usize, no_alloc: bool) -> Result<Self> {
        let raw = unsafe {
            sys::ggml_init(sys::ggml_init_params {
                mem_size: bytes,
                mem_buffer: ptr::null_mut(),
                no_alloc,
            })
        };
        if raw.is_null() {
            bail!("failed to allocate {bytes} bytes of GGML metadata");
        }
        Ok(Self { raw })
    }

    fn tensor(&self, shape: Shape, kind: sys::ggml_type) -> Result<Tensor> {
        let dimensions = shape.ggml_dims();
        let raw =
            unsafe { sys::ggml_new_tensor(self.raw, kind, shape.rank as i32, dimensions.as_ptr()) };
        Tensor::new(raw, shape, kind)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { sys::ggml_free(self.raw) };
    }
}

pub struct Weights {
    _context: Context,
    buffer: sys::ggml_backend_buffer_t,
    tensors: HashMap<String, Tensor>,
}

impl Weights {
    pub fn load(path: &Path, backend: &Backend) -> Result<Self> {
        let path_string = CString::new(path.to_string_lossy().as_bytes())?;
        let mut source_context = ptr::null_mut();
        let gguf = unsafe {
            sys::gguf_init_from_file(
                path_string.as_ptr(),
                sys::gguf_init_params {
                    no_alloc: true,
                    ctx: &mut source_context,
                },
            )
        };
        if gguf.is_null() {
            if !source_context.is_null() {
                unsafe { sys::ggml_free(source_context) };
            }
            bail!("failed to load GGUF model {}", path.display());
        }
        let _gguf_guard = GgufContext { raw: gguf };
        if source_context.is_null() {
            bail!("GGUF model {} has no tensor context", path.display());
        }
        let source = Context {
            raw: source_context,
        };
        let tensor_count = unsafe { sys::gguf_get_n_tensors(gguf) } as usize;
        let logical_names_key =
            unsafe { sys::gguf_find_key(gguf, CString::new("audiocpp.tensor_names")?.as_ptr()) };
        let ranks_key =
            unsafe { sys::gguf_find_key(gguf, CString::new("audiocpp.tensor_ranks")?.as_ptr()) };
        let shapes_key =
            unsafe { sys::gguf_find_key(gguf, CString::new("audiocpp.tensor_shapes")?.as_ptr()) };
        if logical_names_key >= 0
            && (unsafe { sys::gguf_get_kv_type(gguf, logical_names_key) } != sys::GGUF_TYPE_ARRAY
                || unsafe { sys::gguf_get_arr_type(gguf, logical_names_key) }
                    != sys::GGUF_TYPE_STRING
                || unsafe { sys::gguf_get_arr_n(gguf, logical_names_key) } != tensor_count)
        {
            bail!("GGUF logical tensor name metadata is invalid");
        }
        if (ranks_key >= 0) != (shapes_key >= 0) {
            bail!("GGUF exact tensor shape metadata is incomplete");
        }
        let exact_shapes = ranks_key >= 0 && shapes_key >= 0;
        if exact_shapes
            && (unsafe { sys::gguf_get_kv_type(gguf, ranks_key) } != sys::GGUF_TYPE_ARRAY
                || unsafe { sys::gguf_get_arr_type(gguf, ranks_key) } != sys::GGUF_TYPE_INT32
                || unsafe { sys::gguf_get_arr_n(gguf, ranks_key) } != tensor_count
                || unsafe { sys::gguf_get_kv_type(gguf, shapes_key) } != sys::GGUF_TYPE_ARRAY
                || unsafe { sys::gguf_get_arr_type(gguf, shapes_key) } != sys::GGUF_TYPE_INT64)
        {
            bail!("GGUF exact tensor shape metadata is invalid");
        }
        let shape_count = if exact_shapes {
            unsafe { sys::gguf_get_arr_n(gguf, shapes_key) }
        } else {
            0
        };
        let ranks = if exact_shapes {
            unsafe { sys::gguf_get_arr_data(gguf, ranks_key) }.cast::<i32>()
        } else {
            ptr::null()
        };
        let shapes = if exact_shapes {
            unsafe { sys::gguf_get_arr_data(gguf, shapes_key) }.cast::<i64>()
        } else {
            ptr::null()
        };
        if exact_shapes && (ranks.is_null() || shapes.is_null()) {
            bail!("GGUF exact tensor shape arrays are null");
        }
        let mut shape_cursor = 0usize;
        let metadata_bytes = 64 * 1024 * 1024 + tensor_count * 1024;
        let context = Context::new(metadata_bytes, true)?;
        let mut tensors = HashMap::with_capacity(tensor_count);
        let data_offset = unsafe { sys::gguf_get_data_offset(gguf) } as u64;
        let mut model_file = std::fs::File::open(path)?;
        let model_file_length = model_file.metadata()?.len();
        let mut uploads = Vec::with_capacity(tensor_count);

        for index in 0..tensor_count {
            let name_pointer = unsafe { sys::gguf_get_tensor_name(gguf, index as i64) };
            if name_pointer.is_null() {
                bail!("GGUF tensor {index} has no name");
            }
            let physical_name = unsafe { CStr::from_ptr(name_pointer) }
                .to_str()
                .context("GGUF tensor name is not UTF-8")?;
            let logical_name_pointer = if logical_names_key >= 0 {
                unsafe { sys::gguf_get_arr_str(gguf, logical_names_key, index) }
            } else {
                name_pointer
            };
            if logical_name_pointer.is_null() {
                bail!("GGUF logical tensor name {index} is null");
            }
            let name = unsafe { CStr::from_ptr(logical_name_pointer) }
                .to_str()
                .context("GGUF logical tensor name is not UTF-8")?
                .to_owned();
            let source_tensor = unsafe { sys::ggml_get_tensor(source.raw, name_pointer) };
            if source_tensor.is_null() {
                bail!("GGUF tensor {physical_name} is missing from its data context");
            }
            let logical_dimensions = if exact_shapes {
                let rank = unsafe { *ranks.add(index) };
                if !(1..=4).contains(&rank) {
                    bail!("GGUF tensor {name} has invalid rank {rank}");
                }
                let rank = rank as usize;
                let shape_end = shape_cursor
                    .checked_add(rank)
                    .context("GGUF tensor shape cursor overflow")?;
                if shape_end > shape_count {
                    bail!("GGUF tensor {name} shape exceeds metadata array");
                }
                let dimensions =
                    unsafe { std::slice::from_raw_parts(shapes.add(shape_cursor), rank) }.to_vec();
                shape_cursor = shape_end;
                dimensions
            } else {
                let rank = unsafe { sys::ggml_n_dims(source_tensor) } as usize;
                (0..rank)
                    .rev()
                    .map(|axis| unsafe { (*source_tensor).ne[axis] })
                    .collect()
            };
            let shape = Shape::new(&logical_dimensions)?;
            let kind = unsafe { sys::gguf_get_tensor_type(gguf, index as i64) };
            let destination = context.tensor(shape, kind)?;
            let destination_name = CString::new(name.as_str())?;
            unsafe { sys::ggml_set_name(destination.raw, destination_name.as_ptr()) };
            let tensor_offset = unsafe { sys::gguf_get_tensor_offset(gguf, index as i64) } as u64;
            let byte_count = unsafe { sys::gguf_get_tensor_size(gguf, index as i64) };
            let destination_bytes = unsafe { sys::ggml_nbytes(destination.raw) };
            if byte_count != destination_bytes {
                bail!("GGUF tensor {name} has {byte_count} bytes, expected {destination_bytes}");
            }
            let absolute_offset = data_offset
                .checked_add(tensor_offset)
                .context("GGUF tensor file offset overflow")?;
            let byte_count_u64 =
                u64::try_from(byte_count).context("GGUF tensor size exceeds u64")?;
            let tensor_end = absolute_offset
                .checked_add(byte_count_u64)
                .context("GGUF tensor file range overflow")?;
            if tensor_end > model_file_length {
                bail!("GGUF tensor {name} exceeds the model file");
            }
            uploads.push((absolute_offset, byte_count, destination));
            tensors.insert(name, destination);
        }

        if exact_shapes && shape_cursor != shape_count {
            bail!("GGUF tensor shape metadata has trailing dimensions");
        }
        let buffer = unsafe { sys::ggml_backend_alloc_ctx_tensors(context.raw, backend.raw()) };
        if buffer.is_null() {
            bail!("failed to allocate backend weight buffer");
        }
        for (offset, byte_count, destination) in uploads {
            let mut data = vec![0u8; byte_count];
            model_file.seek(SeekFrom::Start(offset))?;
            model_file.read_exact(&mut data)?;
            unsafe {
                sys::ggml_backend_tensor_set(
                    destination.raw,
                    data.as_ptr().cast::<c_void>(),
                    0,
                    byte_count,
                )
            };
        }
        backend.synchronize();
        drop(source);

        Ok(Self {
            _context: context,
            buffer,
            tensors,
        })
    }

    pub fn get(&self, name: &str) -> Result<Tensor> {
        self.tensors
            .get(name)
            .or_else(|| self.tensors.get(&format!("weights/{name}")))
            .copied()
            .ok_or_else(|| anyhow!("model is missing tensor {name}"))
    }

    pub fn len(&self) -> usize {
        self.tensors.len()
    }
}

impl Drop for Weights {
    fn drop(&mut self) {
        unsafe { sys::ggml_backend_buffer_free(self.buffer) };
    }
}

pub struct TensorStorage {
    buffer: sys::ggml_backend_buffer_t,
    tensors: Vec<Tensor>,
    _context: Context,
}

impl TensorStorage {
    pub fn f16(backend: &Backend, shapes: &[Vec<i64>]) -> Result<Self> {
        Self::new(backend, shapes, sys::GGML_TYPE_F16)
    }

    fn new(backend: &Backend, shapes: &[Vec<i64>], kind: sys::ggml_type) -> Result<Self> {
        if shapes.is_empty() {
            bail!("tensor storage requires at least one tensor");
        }
        let context = Context::new(1024 * shapes.len(), true)?;
        let tensors = shapes
            .iter()
            .map(|shape| context.tensor(Shape::new(shape)?, kind))
            .collect::<Result<Vec<_>>>()?;
        let buffer = unsafe { sys::ggml_backend_alloc_ctx_tensors(context.raw, backend.raw()) };
        if buffer.is_null() {
            bail!("failed to allocate tensor storage");
        }
        Ok(Self {
            buffer,
            tensors,
            _context: context,
        })
    }

    pub fn get(&self, index: usize) -> Result<Tensor> {
        self.tensors
            .get(index)
            .copied()
            .ok_or_else(|| anyhow!("tensor storage index {index} is out of range"))
    }

    pub fn clear(&self) {
        unsafe { sys::ggml_backend_buffer_clear(self.buffer, 0) };
    }
}

impl Drop for TensorStorage {
    fn drop(&mut self) {
        unsafe { sys::ggml_backend_buffer_free(self.buffer) };
    }
}

pub struct GraphBuilder {
    context: Context,
    constants: std::cell::RefCell<Vec<(Tensor, Vec<u8>)>>,
}

impl GraphBuilder {
    pub fn new(metadata_bytes: usize) -> Result<Self> {
        Ok(Self {
            context: Context::new(metadata_bytes, true)?,
            constants: std::cell::RefCell::new(Vec::new()),
        })
    }

    fn wrap(
        &self,
        raw: *mut sys::ggml_tensor,
        shape: Shape,
        kind: sys::ggml_type,
    ) -> Result<Tensor> {
        Tensor::new(raw, shape, kind)
    }

    pub fn tensor(&self, shape: &[i64], kind: sys::ggml_type) -> Result<Tensor> {
        self.context.tensor(Shape::new(shape)?, kind)
    }

    pub fn input_f32(&self, shape: &[i64]) -> Result<Tensor> {
        let tensor = self.tensor(shape, sys::GGML_TYPE_F32)?;
        unsafe { sys::ggml_set_input(tensor.raw) };
        Ok(tensor)
    }

    pub fn input_f16(&self, shape: &[i64]) -> Result<Tensor> {
        let tensor = self.tensor(shape, sys::GGML_TYPE_F16)?;
        unsafe { sys::ggml_set_input(tensor.raw) };
        Ok(tensor)
    }

    pub fn input_i32(&self, shape: &[i64]) -> Result<Tensor> {
        let tensor = self.tensor(shape, sys::GGML_TYPE_I32)?;
        unsafe { sys::ggml_set_input(tensor.raw) };
        Ok(tensor)
    }

    pub fn constant_f32(&self, shape: &[i64], values: Vec<f32>) -> Result<Tensor> {
        let tensor = self.tensor(shape, sys::GGML_TYPE_F32)?;
        unsafe { sys::ggml_set_input(tensor.raw) };
        if tensor.shape.elements() != values.len() {
            bail!("constant shape {:?} has {} values", shape, values.len());
        }
        let bytes = unsafe {
            std::slice::from_raw_parts(
                values.as_ptr().cast::<u8>(),
                values.len() * std::mem::size_of::<f32>(),
            )
        }
        .to_vec();
        self.constants.borrow_mut().push((tensor, bytes));
        Ok(tensor)
    }

    pub fn contiguous(&self, value: Tensor) -> Result<Tensor> {
        let contiguous = unsafe { sys::ggml_is_contiguous(value.raw) }
            && unsafe { (*value.raw).view_offs == 0 }
            && unsafe { (*value.raw).nb[0] == sys::ggml_type_size(value.kind) };
        if contiguous {
            return Ok(value);
        }
        self.wrap(
            unsafe { sys::ggml_cont(self.context.raw, value.raw) },
            value.shape,
            value.kind,
        )
    }

    pub fn reshape(&self, value: Tensor, shape: &[i64]) -> Result<Tensor> {
        let shape = Shape::new(shape)?;
        if value.shape.elements() != shape.elements() {
            bail!(
                "cannot reshape {:?} to {:?}",
                value.shape.to_vec(),
                shape.to_vec()
            );
        }
        let dimensions = shape.ggml_dims();
        let raw = match shape.rank {
            1 => unsafe { sys::ggml_reshape_1d(self.context.raw, value.raw, dimensions[0]) },
            2 => unsafe {
                sys::ggml_reshape_2d(self.context.raw, value.raw, dimensions[0], dimensions[1])
            },
            3 => unsafe {
                sys::ggml_reshape_3d(
                    self.context.raw,
                    value.raw,
                    dimensions[0],
                    dimensions[1],
                    dimensions[2],
                )
            },
            4 => unsafe {
                sys::ggml_reshape_4d(
                    self.context.raw,
                    value.raw,
                    dimensions[0],
                    dimensions[1],
                    dimensions[2],
                    dimensions[3],
                )
            },
            _ => unreachable!(),
        };
        self.wrap(raw, shape, value.kind)
    }

    pub fn transpose(&self, value: Tensor, axes: &[usize]) -> Result<Tensor> {
        if axes.len() != value.shape.rank || axes.iter().any(|axis| *axis >= axes.len()) {
            bail!("invalid transpose axes {axes:?}");
        }
        let mut seen = [false; 4];
        for axis in axes {
            if seen[*axis] {
                bail!("duplicate transpose axis {axis}");
            }
            seen[*axis] = true;
        }
        let output_shape = Shape::new(
            &axes
                .iter()
                .map(|axis| value.shape.at(*axis))
                .collect::<Vec<_>>(),
        )?;
        let mut ggml_axes = [0i32, 1, 2, 3];
        for (output_axis, input_axis) in axes.iter().enumerate() {
            let input_ggml = value.shape.rank - 1 - input_axis;
            ggml_axes[input_ggml] = (value.shape.rank - 1 - output_axis) as i32;
        }
        self.wrap(
            unsafe {
                sys::ggml_permute(
                    self.context.raw,
                    value.raw,
                    ggml_axes[0],
                    ggml_axes[1],
                    ggml_axes[2],
                    ggml_axes[3],
                )
            },
            output_shape,
            value.kind,
        )
    }

    pub fn slice(&self, value: Tensor, axis: usize, start: i64, length: i64) -> Result<Tensor> {
        let end = start.checked_add(length);
        if axis >= value.shape.rank
            || start < 0
            || length <= 0
            || end.is_none_or(|end| end > value.shape.at(axis))
        {
            bail!(
                "invalid slice on {:?}: axis={axis}, start={start}, length={length}",
                value.shape.to_vec()
            );
        }
        let shape = value.shape.with_axis(axis, length);
        let dimensions = shape.ggml_dims();
        let ggml_axis = value.shape.rank - 1 - axis;
        let offset = usize::try_from(start)?
            .checked_mul(unsafe { (*value.raw).nb[ggml_axis] })
            .context("tensor slice byte offset overflow")?;
        let raw = match shape.rank {
            1 => unsafe { sys::ggml_view_1d(self.context.raw, value.raw, dimensions[0], offset) },
            2 => unsafe {
                sys::ggml_view_2d(
                    self.context.raw,
                    value.raw,
                    dimensions[0],
                    dimensions[1],
                    (*value.raw).nb[1],
                    offset,
                )
            },
            3 => unsafe {
                sys::ggml_view_3d(
                    self.context.raw,
                    value.raw,
                    dimensions[0],
                    dimensions[1],
                    dimensions[2],
                    (*value.raw).nb[1],
                    (*value.raw).nb[2],
                    offset,
                )
            },
            4 => unsafe {
                sys::ggml_view_4d(
                    self.context.raw,
                    value.raw,
                    dimensions[0],
                    dimensions[1],
                    dimensions[2],
                    dimensions[3],
                    (*value.raw).nb[1],
                    (*value.raw).nb[2],
                    (*value.raw).nb[3],
                    offset,
                )
            },
            _ => unreachable!(),
        };
        self.wrap(raw, shape, value.kind)
    }

    pub fn concat(&self, left: Tensor, right: Tensor, axis: usize) -> Result<Tensor> {
        if left.shape.rank != right.shape.rank || axis >= left.shape.rank {
            bail!("invalid concat ranks");
        }
        for index in 0..left.shape.rank {
            if index != axis && left.shape.at(index) != right.shape.at(index) {
                bail!(
                    "concat shape mismatch {:?} and {:?}",
                    left.shape.to_vec(),
                    right.shape.to_vec()
                );
            }
        }
        let concatenated = left
            .shape
            .at(axis)
            .checked_add(right.shape.at(axis))
            .context("concat dimension overflow")?;
        let shape = left.shape.with_axis(axis, concatenated);
        let dimension = (left.shape.rank - 1 - axis) as i32;
        self.wrap(
            unsafe { sys::ggml_concat(self.context.raw, left.raw, right.raw, dimension) },
            shape,
            left.kind,
        )
    }

    pub fn broadcast(&self, mut value: Tensor, target: Shape) -> Result<Tensor> {
        if value.shape == target {
            return Ok(value);
        }
        if value.shape.rank < target.rank {
            let mut dimensions = vec![1; target.rank - value.shape.rank];
            dimensions.extend(value.shape.to_vec());
            value = self.reshape(self.contiguous(value)?, &dimensions)?;
        }
        if value.shape.rank != target.rank {
            bail!(
                "cannot broadcast {:?} to {:?}",
                value.shape.to_vec(),
                target.to_vec()
            );
        }
        for axis in 0..target.rank {
            if value.shape.at(axis) != 1 && value.shape.at(axis) != target.at(axis) {
                bail!(
                    "cannot broadcast {:?} to {:?}",
                    value.shape.to_vec(),
                    target.to_vec()
                );
            }
        }
        let value = self.contiguous(value)?;
        let template = self.tensor(&target.to_vec(), sys::GGML_TYPE_F32)?;
        self.wrap(
            unsafe { sys::ggml_repeat(self.context.raw, value.raw, template.raw) },
            target,
            value.kind,
        )
    }

    fn broadcast_shape(&self, left: Shape, right: Shape) -> Result<Shape> {
        let rank = left.rank.max(right.rank);
        let mut dimensions = vec![1; rank];
        for axis in 0..rank {
            let left_dimension = if axis < rank - left.rank {
                1
            } else {
                left.at(axis - (rank - left.rank))
            };
            let right_dimension = if axis < rank - right.rank {
                1
            } else {
                right.at(axis - (rank - right.rank))
            };
            if left_dimension != right_dimension && left_dimension != 1 && right_dimension != 1 {
                bail!(
                    "broadcast mismatch {:?} and {:?}",
                    left.to_vec(),
                    right.to_vec()
                );
            }
            dimensions[axis] = left_dimension.max(right_dimension);
        }
        Shape::new(&dimensions)
    }

    fn binary(
        &self,
        left: Tensor,
        right: Tensor,
        operation: unsafe extern "C" fn(
            *mut sys::ggml_context,
            *mut sys::ggml_tensor,
            *mut sys::ggml_tensor,
        ) -> *mut sys::ggml_tensor,
    ) -> Result<Tensor> {
        let shape = self.broadcast_shape(left.shape, right.shape)?;
        if left.shape == shape {
            let left = self.contiguous(left)?;
            let right = self.contiguous(right)?;
            if unsafe { sys::ggml_can_repeat(right.raw, left.raw) } {
                return self.wrap(
                    unsafe { operation(self.context.raw, left.raw, right.raw) },
                    shape,
                    sys::GGML_TYPE_F32,
                );
            }
        }
        let left = self.contiguous(self.broadcast(left, shape)?)?;
        let right = self.contiguous(self.broadcast(right, shape)?)?;
        self.wrap(
            unsafe { operation(self.context.raw, left.raw, right.raw) },
            shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn add(&self, left: Tensor, right: Tensor) -> Result<Tensor> {
        self.binary(left, right, sys::ggml_add)
    }

    pub fn sub(&self, left: Tensor, right: Tensor) -> Result<Tensor> {
        self.binary(left, right, sys::ggml_sub)
    }

    pub fn mul(&self, left: Tensor, right: Tensor) -> Result<Tensor> {
        self.binary(left, right, sys::ggml_mul)
    }

    pub fn div(&self, left: Tensor, right: Tensor) -> Result<Tensor> {
        self.binary(left, right, sys::ggml_div)
    }

    pub fn scale(&self, value: Tensor, scale: f32) -> Result<Tensor> {
        let value = self.contiguous(value)?;
        self.wrap(
            unsafe { sys::ggml_scale(self.context.raw, value.raw, scale) },
            value.shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn scale_bias(&self, value: Tensor, scale: f32, bias: f32) -> Result<Tensor> {
        let value = self.contiguous(value)?;
        self.wrap(
            unsafe { sys::ggml_scale_bias(self.context.raw, value.raw, scale, bias) },
            value.shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn unary(
        &self,
        value: Tensor,
        operation: unsafe extern "C" fn(
            *mut sys::ggml_context,
            *mut sys::ggml_tensor,
        ) -> *mut sys::ggml_tensor,
    ) -> Result<Tensor> {
        let value = self.contiguous(value)?;
        self.wrap(
            unsafe { operation(self.context.raw, value.raw) },
            value.shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn exp(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_exp)
    }
    pub fn sqrt(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_sqrt)
    }
    pub fn tanh(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_tanh)
    }
    pub fn sin(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_sin)
    }
    pub fn cos(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_cos)
    }
    pub fn relu(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_relu)
    }
    pub fn gelu(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_gelu_erf)
    }
    pub fn gelu_tanh(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_gelu)
    }
    pub fn geglu(&self, gate: Tensor, up: Tensor) -> Result<Tensor> {
        if gate.shape != up.shape {
            bail!("GEGLU input shape mismatch");
        }
        let gate = self.contiguous(gate)?;
        let up = self.contiguous(up)?;
        self.wrap(
            unsafe { sys::ggml_geglu_split(self.context.raw, gate.raw, up.raw) },
            gate.shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn softplus(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_softplus)
    }
    pub fn sigmoid(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_sigmoid)
    }
    pub fn silu(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_silu)
    }
    pub fn softmax(&self, value: Tensor) -> Result<Tensor> {
        self.unary(value, sys::ggml_soft_max)
    }
    pub fn rms_norm(&self, value: Tensor, epsilon: f32) -> Result<Tensor> {
        let value = self.contiguous(value)?;
        self.wrap(
            unsafe { sys::ggml_rms_norm(self.context.raw, value.raw, epsilon) },
            value.shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn rope_neox(
        &self,
        value: Tensor,
        positions: Tensor,
        rotated_dimensions: usize,
        theta: f32,
        original_context: usize,
        frequency_factors: Option<Tensor>,
    ) -> Result<Tensor> {
        let invalid_factors = frequency_factors.is_some_and(|factors| {
            factors.shape.rank != 1
                || factors.shape.at(0) != rotated_dimensions as i64 / 2
                || factors.kind != sys::GGML_TYPE_F32
        });
        if value.shape.rank != 4
            || positions.shape.rank != 1
            || value.shape.at(1) != positions.shape.at(0)
            || positions.kind != sys::GGML_TYPE_I32
            || invalid_factors
            || rotated_dimensions == 0
            || rotated_dimensions > value.shape.last() as usize
            || rotated_dimensions % 2 != 0
        {
            bail!("invalid NeoX RoPE inputs");
        }
        let value = self.contiguous(value)?;
        let frequency_factors = frequency_factors.map_or(ptr::null_mut(), |tensor| tensor.raw);
        self.wrap(
            unsafe {
                sys::ggml_rope_ext(
                    self.context.raw,
                    value.raw,
                    positions.raw,
                    frequency_factors,
                    rotated_dimensions as i32,
                    sys::GGML_ROPE_TYPE_NEOX as i32,
                    original_context as i32,
                    theta,
                    1.0,
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                )
            },
            value.shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn flash_attention(
        &self,
        query: Tensor,
        key: Tensor,
        value: Tensor,
        mask: Tensor,
        scale: f32,
    ) -> Result<Tensor> {
        if query.shape.rank != 4
            || key.shape.rank != 4
            || value.shape.rank != 4
            || mask.shape.rank != 4
            || query.kind != sys::GGML_TYPE_F32
            || key.kind != sys::GGML_TYPE_F16
            || value.kind != sys::GGML_TYPE_F16
            || mask.kind != sys::GGML_TYPE_F16
            || query.shape.at(0) != key.shape.at(0)
            || key.shape.at(0) != value.shape.at(0)
            || query.shape.last() != key.shape.last()
            || key.shape.at(1) != value.shape.at(1)
            || key.shape.at(2) != value.shape.at(2)
            || query.shape.at(1) % key.shape.at(1) != 0
            || query.shape.at(0) % mask.shape.at(0) != 0
            || query.shape.at(1) % mask.shape.at(1) != 0
            || mask.shape.at(2) != query.shape.at(2)
            || mask.shape.last() != key.shape.at(2)
        {
            bail!("invalid flash-attention inputs");
        }
        let raw = unsafe {
            sys::ggml_flash_attn_ext(
                self.context.raw,
                query.raw,
                key.raw,
                value.raw,
                mask.raw,
                scale,
                0.0,
                0.0,
            )
        };
        unsafe { sys::ggml_flash_attn_ext_set_prec(raw, sys::GGML_PREC_F32) };
        self.wrap(
            raw,
            Shape::new(&[
                query.shape.at(0),
                query.shape.at(2),
                query.shape.at(1),
                value.shape.last(),
            ])?,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn cast_f32(&self, value: Tensor) -> Result<Tensor> {
        if value.kind == sys::GGML_TYPE_F32 {
            return Ok(value);
        }
        self.wrap(
            unsafe { sys::ggml_cast(self.context.raw, value.raw, sys::GGML_TYPE_F32) },
            value.shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn prelu(&self, value: Tensor, slope: Tensor) -> Result<Tensor> {
        let positive = self.relu(value)?;
        let negative = self.sub(value, positive)?;
        self.add(positive, self.mul(negative, slope)?)
    }

    pub fn layer_norm(
        &self,
        value: Tensor,
        weight: Tensor,
        bias: Tensor,
        epsilon: f32,
    ) -> Result<Tensor> {
        let value = self.contiguous(value)?;
        let normalized = self.wrap(
            unsafe { sys::ggml_norm(self.context.raw, value.raw, epsilon) },
            value.shape,
            sys::GGML_TYPE_F32,
        )?;
        self.add(self.mul(normalized, weight)?, bias)
    }

    pub fn matmul(&self, left: Tensor, right: Tensor) -> Result<Tensor> {
        if left.shape.rank < 2
            || left.shape.rank != right.shape.rank
            || left.shape.last() != right.shape.at(right.shape.rank - 2)
        {
            bail!(
                "matmul mismatch {:?} x {:?}",
                left.shape.to_vec(),
                right.shape.to_vec()
            );
        }
        for axis in 0..left.shape.rank - 2 {
            if left.shape.at(axis) != right.shape.at(axis) {
                bail!("matmul batch mismatch");
            }
        }
        let mut axes: Vec<usize> = (0..right.shape.rank).collect();
        axes.swap(right.shape.rank - 1, right.shape.rank - 2);
        let transposed = self.contiguous(self.transpose(right, &axes)?)?;
        let left = self.contiguous(left)?;
        let shape = left
            .shape
            .with_axis(left.shape.rank - 1, right.shape.last());
        self.wrap(
            unsafe { sys::ggml_mul_mat(self.context.raw, transposed.raw, left.raw) },
            shape,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn linear(&self, input: Tensor, weight: Tensor, bias: Option<Tensor>) -> Result<Tensor> {
        if weight.shape.rank != 2 || input.shape.last() != weight.shape.at(1) {
            bail!(
                "linear mismatch {:?} and {:?}",
                input.shape.to_vec(),
                weight.shape.to_vec()
            );
        }
        let output_shape = input
            .shape
            .with_axis(input.shape.rank - 1, weight.shape.at(0));
        let input = self.contiguous(input)?;
        let weight = self.contiguous(weight)?;
        let output = self.wrap(
            unsafe { sys::ggml_mul_mat(self.context.raw, weight.raw, input.raw) },
            output_shape,
            sys::GGML_TYPE_F32,
        )?;
        match bias {
            Some(bias) => self.add(output, bias),
            None => Ok(output),
        }
    }

    pub fn set_rows(&self, destination: Tensor, source: Tensor, indices: Tensor) -> Result<Tensor> {
        if destination.shape.rank != 4 || source.shape.rank != 4 {
            bail!("set_rows requires rank-4 destination and source tensors");
        }
        if !matches!(destination.kind, sys::GGML_TYPE_F16 | sys::GGML_TYPE_F32)
            || !matches!(source.kind, sys::GGML_TYPE_F16 | sys::GGML_TYPE_F32)
            || indices.kind != sys::GGML_TYPE_I32
        {
            bail!("set_rows requires F16/F32 destination/source and I32 indices");
        }
        if indices.shape.rank != 1 {
            bail!("set_rows indices must be one-dimensional");
        }
        if destination.shape.at(0) != source.shape.at(0)
            || destination.shape.at(1) != source.shape.at(1)
            || destination.shape.last() != source.shape.last()
        {
            bail!("set_rows destination/source shape mismatch");
        }
        if source.shape.at(2) != indices.shape.at(0) {
            bail!("set_rows source sequence count differs from indices");
        }
        let source = self.contiguous(source)?;
        self.wrap(
            unsafe {
                sys::ggml_set_rows(self.context.raw, destination.raw, source.raw, indices.raw)
            },
            destination.shape,
            destination.kind,
        )
    }

    pub fn embedding(&self, ids: Tensor, table: Tensor) -> Result<Tensor> {
        let mut dimensions = ids.shape.to_vec();
        dimensions.push(table.shape.at(1));
        self.wrap(
            unsafe { sys::ggml_get_rows(self.context.raw, table.raw, ids.raw) },
            Shape::new(&dimensions)?,
            sys::GGML_TYPE_F32,
        )
    }

    pub fn conv1d(
        &self,
        input: Tensor,
        weight: Tensor,
        bias: Option<Tensor>,
        dilation: i32,
    ) -> Result<Tensor> {
        if input.shape.rank != 3
            || weight.shape.rank != 3
            || input.shape.at(1) != weight.shape.at(1)
        {
            bail!(
                "conv1d mismatch {:?} and {:?}",
                input.shape.to_vec(),
                weight.shape.to_vec()
            );
        }
        let frames = input.shape.at(2) - dilation as i64 * (weight.shape.at(2) - 1);
        let output_shape = Shape::new(&[input.shape.at(0), weight.shape.at(0), frames])?;
        let input = self.contiguous(input)?;
        let weight = self.contiguous(weight)?;
        let mut output: Option<Tensor> = None;
        for batch in 0..input.shape.at(0) {
            let batch_input = self.slice(input, 0, batch, 1)?;
            let batch_input = self.reshape(batch_input, &[input.shape.at(1), input.shape.at(2)])?;
            let batch_output = self.wrap(
                unsafe {
                    sys::ggml_conv_1d(
                        self.context.raw,
                        weight.raw,
                        batch_input.raw,
                        1,
                        0,
                        dilation,
                    )
                },
                Shape::new(&[1, weight.shape.at(0), frames])?,
                sys::GGML_TYPE_F32,
            )?;
            output = Some(match output {
                Some(previous) => self.concat(previous, batch_output, 0)?,
                None => batch_output,
            });
        }
        let output = output.ok_or_else(|| anyhow!("empty convolution batch"))?;
        let output = match bias {
            Some(bias) => self.add(output, self.reshape(bias, &[1, output_shape.at(1), 1])?)?,
            None => output,
        };
        Ok(output)
    }

    pub fn depthwise_conv1d(
        &self,
        input: Tensor,
        weight: Tensor,
        bias: Option<Tensor>,
        dilation: i32,
    ) -> Result<Tensor> {
        if input.shape.rank != 3
            || weight.shape.rank != 3
            || input.shape.at(1) != weight.shape.at(0)
        {
            bail!(
                "depthwise conv mismatch {:?} and {:?}",
                input.shape.to_vec(),
                weight.shape.to_vec()
            );
        }
        let frames = input.shape.at(2) - dilation as i64 * (weight.shape.at(2) - 1);
        let output_shape = Shape::new(&[input.shape.at(0), input.shape.at(1), frames])?;
        let input = self.contiguous(input)?;
        let weight = self.contiguous(weight)?;
        let mut output: Option<Tensor> = None;
        for batch in 0..input.shape.at(0) {
            let batch_input = self.slice(input, 0, batch, 1)?;
            let batch_input = self.reshape(batch_input, &[input.shape.at(1), input.shape.at(2)])?;
            let batch_output = self.wrap(
                unsafe {
                    sys::ggml_conv_1d_dw(
                        self.context.raw,
                        weight.raw,
                        batch_input.raw,
                        1,
                        0,
                        dilation,
                    )
                },
                Shape::new(&[1, input.shape.at(1), frames])?,
                sys::GGML_TYPE_F32,
            )?;
            output = Some(match output {
                Some(previous) => self.concat(previous, batch_output, 0)?,
                None => batch_output,
            });
        }
        let output = output.ok_or_else(|| anyhow!("empty depthwise convolution batch"))?;
        match bias {
            Some(bias) => self.add(output, self.reshape(bias, &[1, output_shape.at(1), 1])?),
            None => Ok(output),
        }
    }

    pub fn reduce_sum(&self, input: Tensor, axis: usize) -> Result<Tensor> {
        if axis >= input.shape.rank {
            bail!("reduction axis is out of range");
        }
        if axis == input.shape.rank - 1 {
            let input = self.contiguous(input)?;
            return self.wrap(
                unsafe { sys::ggml_sum_rows(self.context.raw, input.raw) },
                input.shape.with_axis(axis, 1),
                sys::GGML_TYPE_F32,
            );
        }
        let mut axes: Vec<usize> = (0..input.shape.rank).collect();
        axes.swap(axis, input.shape.rank - 1);
        let transposed = self.contiguous(self.transpose(input, &axes)?)?;
        let reduced = self.wrap(
            unsafe { sys::ggml_sum_rows(self.context.raw, transposed.raw) },
            transposed.shape.with_axis(transposed.shape.rank - 1, 1),
            sys::GGML_TYPE_F32,
        )?;
        self.transpose(reduced, &axes)
    }

    pub fn edge_pad(&self, input: Tensor, left: i64, right: i64, axis: usize) -> Result<Tensor> {
        let mut output = input;
        if left > 0 {
            let edge = self.slice(input, axis, 0, 1)?;
            let target = edge.shape.with_axis(axis, left);
            output = self.concat(self.broadcast(edge, target)?, output, axis)?;
        }
        if right > 0 {
            let edge = self.slice(input, axis, input.shape.at(axis) - 1, 1)?;
            let target = edge.shape.with_axis(axis, right);
            output = self.concat(output, self.broadcast(edge, target)?, axis)?;
        }
        Ok(output)
    }

    pub fn finish(self, output: Tensor, backend: &Backend) -> Result<Graph> {
        unsafe { sys::ggml_set_output(output.raw) };
        let raw = unsafe { sys::ggml_new_graph_custom(self.context.raw, 65_536, false) };
        if raw.is_null() {
            bail!("failed to create GGML graph");
        }
        unsafe { sys::ggml_build_forward_expand(raw, output.raw) };
        let allocator = unsafe {
            sys::ggml_gallocr_new(sys::ggml_backend_get_default_buffer_type(backend.raw()))
        };
        if allocator.is_null() {
            bail!("failed to create GGML graph allocator");
        }
        let reserved = unsafe { sys::ggml_gallocr_reserve(allocator, raw) };
        let allocated = reserved && unsafe { sys::ggml_gallocr_alloc_graph(allocator, raw) };
        if !allocated {
            unsafe { sys::ggml_gallocr_free(allocator) };
            bail!("failed to allocate GGML graph");
        }
        for (tensor, bytes) in self.constants.into_inner() {
            unsafe {
                sys::ggml_backend_tensor_set(
                    tensor.raw,
                    bytes.as_ptr().cast::<c_void>(),
                    0,
                    bytes.len(),
                )
            };
        }
        Ok(Graph {
            _context: self.context,
            raw,
            allocator,
            output,
        })
    }
}

pub struct Graph {
    _context: Context,
    raw: *mut sys::ggml_cgraph,
    allocator: sys::ggml_gallocr_t,
    output: Tensor,
}

impl Graph {
    pub fn set_f32(&self, tensor: Tensor, values: &[f32]) -> Result<()> {
        self.set_bytes(tensor, unsafe {
            std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
        })
    }

    pub fn set_i32(&self, tensor: Tensor, values: &[i32]) -> Result<()> {
        self.set_bytes(tensor, unsafe {
            std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
        })
    }

    pub fn set_f16_bits(&self, tensor: Tensor, values: &[u16]) -> Result<()> {
        self.set_bytes(tensor, unsafe {
            std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
        })
    }

    fn set_bytes(&self, tensor: Tensor, bytes: &[u8]) -> Result<()> {
        let expected = unsafe { sys::ggml_nbytes(tensor.raw) };
        if expected != bytes.len() {
            bail!("input requires {expected} bytes, received {}", bytes.len());
        }
        unsafe {
            sys::ggml_backend_tensor_set(
                tensor.raw,
                bytes.as_ptr().cast::<c_void>(),
                0,
                bytes.len(),
            )
        };
        Ok(())
    }

    pub fn compute(&self, backend: &Backend) -> Result<()> {
        let status = unsafe { sys::ggml_backend_graph_compute(backend.raw(), self.raw) };
        if status != sys::GGML_STATUS_SUCCESS {
            bail!("GGML graph failed with status {status}");
        }
        Ok(())
    }

    pub fn output_f32(&self) -> Result<Vec<f32>> {
        let mut output = vec![0.0; self.output.shape.elements()];
        unsafe {
            sys::ggml_backend_tensor_get(
                self.output.raw,
                output.as_mut_ptr().cast::<c_void>(),
                0,
                std::mem::size_of_val(output.as_slice()),
            )
        };
        Ok(output)
    }
}

impl Drop for Graph {
    fn drop(&mut self) {
        unsafe { sys::ggml_gallocr_free(self.allocator) };
    }
}
