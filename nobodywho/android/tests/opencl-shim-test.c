#define CL_TARGET_OPENCL_VERSION 300
#define CL_USE_DEPRECATED_OPENCL_1_2_APIS
#include <CL/cl.h>
#include <CL/cl_ext.h>
#include <assert.h>
#include <stdint.h>
#include <string.h>

#ifdef MOCK_DRIVER
// The mock deliberately has no ICD dispatch tables.
#define clCreateSubBuffer unused_subbuffer
#ifdef OMIT_REQUIRED
#define clFinish unused_finish
#endif
#ifdef OMIT_OPTIONAL
#define clCreateBufferWithProperties unused_buffer_properties
#define clGetKernelSubGroupInfo unused_subgroup_info
#endif
#define RETURN_PLATFORM do { if (count) *count = 1; return CL_SUCCESS; } while (0)
#define RETURN_INT return CL_SUCCESS
#define RETURN_HANDLE do { if (errcode_ret) *errcode_ret = CL_SUCCESS; return NULL; } while (0)
#define X(required, kind, result, name, params, args) \
    CL_API_ENTRY result CL_API_CALL name params { RETURN_##kind; }
#include "../opencl-functions.inc"
#undef X
#undef clCreateSubBuffer
CL_API_ENTRY cl_mem CL_API_CALL clCreateSubBuffer(cl_mem buffer, cl_mem_flags flags,
    cl_buffer_create_type type, const void *info, cl_int *error) {
    const cl_buffer_region *region = info;
    assert(buffer == (cl_mem)(uintptr_t)0x1234);
    assert(flags == CL_MEM_READ_WRITE && type == CL_BUFFER_CREATE_TYPE_REGION);
    assert(region->origin == 128 && region->size == 1024);
    *error = CL_SUCCESS;
    return (cl_mem)(uintptr_t)0x5678;
}
#else
int main(int argc, char **argv) {
    assert(argc == 2);
    cl_uint count = 999;
    cl_int error = -99999;
    cl_buffer_region region = {128, 1024};
    if (!strcmp(argv[1], "unavailable")) {
        assert(clGetPlatformIDs(0, NULL, &count) == CL_PLATFORM_NOT_FOUND_KHR);
        assert(count == 0);
        assert(clCreateSubBuffer(NULL, 0, 0, NULL, &error) == NULL);
        assert(error == CL_INVALID_OPERATION);
        assert(clFinish(NULL) == CL_INVALID_OPERATION);
    } else {
        assert(clGetPlatformIDs(0, NULL, &count) == CL_SUCCESS && count == 1);
        assert(clCreateSubBuffer((cl_mem)(uintptr_t)0x1234, CL_MEM_READ_WRITE,
            CL_BUFFER_CREATE_TYPE_REGION, &region, &error) == (cl_mem)(uintptr_t)0x5678);
        assert(error == CL_SUCCESS);
        if (!strcmp(argv[1], "optional")) {
            assert(clCreateBufferWithProperties(NULL, NULL, 0, 0, NULL, &error) == NULL);
            assert(error == CL_INVALID_OPERATION);
            assert(clGetKernelSubGroupInfo(NULL, NULL, 0, 0, NULL, 0, NULL, NULL) == CL_INVALID_OPERATION);
        }
    }
    return 0;
}
#endif
