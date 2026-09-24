// Required OpenCL 1.2 API plus optional newer calls used by ggml-opencl.
// X(required, return-kind, return-type, name, declaration, arguments)
X(1, PLATFORM, cl_int, clGetPlatformIDs,
  (cl_uint n, cl_platform_id *platforms, cl_uint *count), (n, platforms, count))
X(1, INT, cl_int, clGetPlatformInfo,
  (cl_platform_id p, cl_platform_info key, size_t size, void *value, size_t *actual), (p, key, size, value, actual))
X(1, INT, cl_int, clGetDeviceIDs,
  (cl_platform_id p, cl_device_type type, cl_uint n, cl_device_id *devices, cl_uint *count), (p, type, n, devices, count))
X(1, INT, cl_int, clGetDeviceInfo,
  (cl_device_id d, cl_device_info key, size_t size, void *value, size_t *actual), (d, key, size, value, actual))
X(1, HANDLE, cl_context, clCreateContext,
  (const cl_context_properties *props, cl_uint n, const cl_device_id *devices, void (CL_CALLBACK *notify)(const char *, const void *, size_t, void *), void *user, cl_int *errcode_ret), (props, n, devices, notify, user, errcode_ret))
X(1, INT, cl_int, clReleaseContext, (cl_context c), (c))
X(1, HANDLE, cl_command_queue, clCreateCommandQueue,
  (cl_context c, cl_device_id d, cl_command_queue_properties props, cl_int *errcode_ret), (c, d, props, errcode_ret))
X(1, INT, cl_int, clReleaseCommandQueue, (cl_command_queue q), (q))
X(1, HANDLE, cl_mem, clCreateBuffer,
  (cl_context c, cl_mem_flags flags, size_t size, void *host, cl_int *errcode_ret), (c, flags, size, host, errcode_ret))
X(0, HANDLE, cl_mem, clCreateBufferWithProperties,
  (cl_context c, const cl_mem_properties *props, cl_mem_flags flags, size_t size, void *host, cl_int *errcode_ret), (c, props, flags, size, host, errcode_ret))
X(1, HANDLE, cl_mem, clCreateSubBuffer,
  (cl_mem b, cl_mem_flags flags, cl_buffer_create_type type, const void *region, cl_int *errcode_ret), (b, flags, type, region, errcode_ret))
X(1, HANDLE, cl_mem, clCreateImage,
  (cl_context c, cl_mem_flags flags, const cl_image_format *format, const cl_image_desc *desc, void *host, cl_int *errcode_ret), (c, flags, format, desc, host, errcode_ret))
X(1, INT, cl_int, clGetMemObjectInfo,
  (cl_mem b, cl_mem_info key, size_t size, void *value, size_t *actual), (b, key, size, value, actual))
X(1, INT, cl_int, clReleaseMemObject, (cl_mem b), (b))
X(1, HANDLE, cl_program, clCreateProgramWithSource,
  (cl_context c, cl_uint n, const char **strings, const size_t *lengths, cl_int *errcode_ret), (c, n, strings, lengths, errcode_ret))
X(1, HANDLE, cl_program, clCreateProgramWithBinary,
  (cl_context c, cl_uint n, const cl_device_id *devices, const size_t *lengths, const unsigned char **binaries, cl_int *binary_status, cl_int *errcode_ret), (c, n, devices, lengths, binaries, binary_status, errcode_ret))
X(1, INT, cl_int, clBuildProgram,
  (cl_program p, cl_uint n, const cl_device_id *devices, const char *options, void (CL_CALLBACK *notify)(cl_program, void *), void *user), (p, n, devices, options, notify, user))
X(1, INT, cl_int, clGetProgramInfo,
  (cl_program p, cl_program_info key, size_t size, void *value, size_t *actual), (p, key, size, value, actual))
X(1, INT, cl_int, clGetProgramBuildInfo,
  (cl_program p, cl_device_id d, cl_program_build_info key, size_t size, void *value, size_t *actual), (p, d, key, size, value, actual))
X(1, INT, cl_int, clReleaseProgram, (cl_program p), (p))
X(1, HANDLE, cl_kernel, clCreateKernel,
  (cl_program p, const char *name, cl_int *errcode_ret), (p, name, errcode_ret))
X(1, INT, cl_int, clSetKernelArg,
  (cl_kernel k, cl_uint index, size_t size, const void *value), (k, index, size, value))
X(1, INT, cl_int, clGetKernelInfo,
  (cl_kernel k, cl_kernel_info key, size_t size, void *value, size_t *actual), (k, key, size, value, actual))
X(1, INT, cl_int, clGetKernelWorkGroupInfo,
  (cl_kernel k, cl_device_id d, cl_kernel_work_group_info key, size_t size, void *value, size_t *actual), (k, d, key, size, value, actual))
X(0, INT, cl_int, clGetKernelSubGroupInfo,
  (cl_kernel k, cl_device_id d, cl_kernel_sub_group_info key, size_t input_size, const void *input, size_t size, void *value, size_t *actual), (k, d, key, input_size, input, size, value, actual))
X(1, INT, cl_int, clReleaseKernel, (cl_kernel k), (k))
X(1, INT, cl_int, clEnqueueReadBuffer,
  (cl_command_queue q, cl_mem b, cl_bool blocking, size_t offset, size_t size, void *ptr, cl_uint n, const cl_event *wait, cl_event *event), (q, b, blocking, offset, size, ptr, n, wait, event))
X(1, INT, cl_int, clEnqueueWriteBuffer,
  (cl_command_queue q, cl_mem b, cl_bool blocking, size_t offset, size_t size, const void *ptr, cl_uint n, const cl_event *wait, cl_event *event), (q, b, blocking, offset, size, ptr, n, wait, event))
X(1, INT, cl_int, clEnqueueCopyBuffer,
  (cl_command_queue q, cl_mem src, cl_mem dst, size_t src_offset, size_t dst_offset, size_t size, cl_uint n, const cl_event *wait, cl_event *event), (q, src, dst, src_offset, dst_offset, size, n, wait, event))
X(1, INT, cl_int, clEnqueueFillBuffer,
  (cl_command_queue q, cl_mem b, const void *pattern, size_t pattern_size, size_t offset, size_t size, cl_uint n, const cl_event *wait, cl_event *event), (q, b, pattern, pattern_size, offset, size, n, wait, event))
X(1, HANDLE, void *, clEnqueueMapBuffer,
  (cl_command_queue q, cl_mem b, cl_bool blocking, cl_map_flags flags, size_t offset, size_t size, cl_uint n, const cl_event *wait, cl_event *event, cl_int *errcode_ret), (q, b, blocking, flags, offset, size, n, wait, event, errcode_ret))
X(1, INT, cl_int, clEnqueueUnmapMemObject,
  (cl_command_queue q, cl_mem b, void *ptr, cl_uint n, const cl_event *wait, cl_event *event), (q, b, ptr, n, wait, event))
X(1, INT, cl_int, clEnqueueNDRangeKernel,
  (cl_command_queue q, cl_kernel k, cl_uint dimensions, const size_t *offset, const size_t *global, const size_t *local, cl_uint n, const cl_event *wait, cl_event *event), (q, k, dimensions, offset, global, local, n, wait, event))
X(1, INT, cl_int, clEnqueueBarrierWithWaitList,
  (cl_command_queue q, cl_uint n, const cl_event *wait, cl_event *event), (q, n, wait, event))
X(1, INT, cl_int, clEnqueueMarkerWithWaitList,
  (cl_command_queue q, cl_uint n, const cl_event *wait, cl_event *event), (q, n, wait, event))
X(1, INT, cl_int, clWaitForEvents, (cl_uint n, const cl_event *events), (n, events))
X(1, INT, cl_int, clGetEventProfilingInfo,
  (cl_event e, cl_profiling_info key, size_t size, void *value, size_t *actual), (e, key, size, value, actual))
X(1, INT, cl_int, clReleaseEvent, (cl_event e), (e))
X(1, INT, cl_int, clFinish, (cl_command_queue q), (q))
X(1, INT, cl_int, clFlush, (cl_command_queue q), (q))
