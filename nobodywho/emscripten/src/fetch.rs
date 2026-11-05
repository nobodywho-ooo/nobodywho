//! Rusty API for emscripten/fetch.h
//!
//! <https://github.com/emscripten-core/emscripten/blob/6.0.13/system/include/emscripten/fetch.h>
#![allow(dead_code, non_snake_case)]

use core::slice;
use std::{
    ffi::{CStr, c_char, c_int, c_ushort, c_void},
    ptr::null_mut,
};

pub struct FetchBuilder {
    attr: emscripten_fetch_attr_t,
}

impl FetchBuilder {
    pub fn new(method: &str) -> Self {
        let mut attr = emscripten_fetch_attr_t::default();
        unsafe { emscripten_fetch_attr_init(&mut attr) };
        assert!(method.len() < 32);
        for (i, x) in method.as_bytes().iter().enumerate() {
            attr.requestMethod[i] = *x as c_char;
        }
        Self { attr }
    }

    pub fn build(&self, url: &CStr) -> Fetch {
        let attr: *const emscripten_fetch_attr_t = &self.attr;
        let mut fetch = Fetch { ptr: null_mut() };
        fetch.ptr = unsafe { emscripten_fetch(attr.cast_mut(), url.as_ptr()) };
        fetch
    }
}

pub struct Fetch {
    ptr: *mut emscripten_fetch_t,
}

impl Drop for Fetch {
    fn drop(&mut self) {
        eprintln!("drop");
        // let res = unsafe { emscripten_fetch_close(self.ptr) };
        // assert_eq!(res, 0);
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    #[repr(transparent)]
    struct FetchAttributes: u32 {
        /// Emscripten fetch attributes:
        /// If passed, the body of the request will be present in full in the onsuccess()
        /// handler.
        #[doc(alias = "EMSCRIPTEN_FETCH_LOAD_TO_MEMORY")]
        const LOAD_TO_MEMORY = 1;

        /// If passed, the intermediate streamed bytes will be passed in to the
        /// onprogress() handler. If not specified, the onprogress() handler will still
        /// be called, but without data bytes.  Note: Firefox only as it depends on
        /// 'moz-chunked-arraybuffer'.
        #[doc(alias = "EMSCRIPTEN_FETCH_STREAM_DATA")]
        const STREAM_DATA = 2;

        /// If passed, the final download will be stored in IndexedDB. If not specified,
        /// the file will only reside in browser memory.
        #[doc(alias = "EMSCRIPTEN_FETCH_PERSIST_FILE")]
        const PERSIST_FILE = 4;

        /// Looks up if the file already exists in IndexedDB, and if so, it is returned
        /// without redownload. If a partial transfer exists in IndexedDB, the download
        /// will resume from where it left off and run to completion.
        /// EMSCRIPTEN_FETCH_APPEND, EMSCRIPTEN_FETCH_REPLACE and
        /// EMSCRIPTEN_FETCH_NO_DOWNLOAD are mutually exclusive.  If none of these three
        /// flags is specified, the fetch operation is implicitly treated as if
        /// EMSCRIPTEN_FETCH_APPEND had been passed.
        #[doc(alias = "EMSCRIPTEN_FETCH_APPEND")]
        const APPEND = 8;

        /// If the file already exists in IndexedDB, the old file will be deleted and a
        /// new download is started.
        /// EMSCRIPTEN_FETCH_APPEND, EMSCRIPTEN_FETCH_REPLACE and
        /// EMSCRIPTEN_FETCH_NO_DOWNLOAD are mutually exclusive.  If you would like to
        /// perform an XHR that neither reads or writes to IndexedDB, pass this flag
        /// EMSCRIPTEN_FETCH_REPLACE, and do not pass the flag
        /// EMSCRIPTEN_FETCH_PERSIST_FILE.
        #[doc(alias = "EMSCRIPTEN_FETCH_REPLACE")]
        const REPLACE = 16;

        /// If specified, the file will only be looked up in IndexedDB, but if it does
        /// not exist, it is not attempted to be downloaded over the network but an error
        /// is raised.
        /// EMSCRIPTEN_FETCH_APPEND, EMSCRIPTEN_FETCH_REPLACE and
        /// EMSCRIPTEN_FETCH_NO_DOWNLOAD are mutually exclusive.
        #[doc(alias = "EMSCRIPTEN_FETCH_NO_DOWNLOAD")]
        const NO_DOWNLOAD = 32;

        /// If specified, emscripten_fetch() will synchronously run to completion before
        /// returning.  The callback handlers will be called from within
        /// emscripten_fetch() while the operation is in progress.
        #[doc(alias = "EMSCRIPTEN_FETCH_SYNCHRONOUS")]
        const SYNCHRONOUS = 64;

        #[deprecated = "waitable fetch requests are no longer implemented"]
        #[doc(alias = "EMSCRIPTEN_FETCH_WAITABLE")]
        const WAITABLE = 128;
    }
}

/// Specifies the parameters for a newly initiated fetch operation.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
struct emscripten_fetch_attr_t {
    /// 'POST', 'GET', etc.
    requestMethod: [c_char; 32usize],

    /// Custom data that can be tagged along the process.
    userData: *mut c_void,

    onsuccess: Option<unsafe extern "C-unwind" fn(fetch: *mut emscripten_fetch_t)>,
    onerror: Option<unsafe extern "C-unwind" fn(fetch: *mut emscripten_fetch_t)>,
    onprogress: Option<unsafe extern "C-unwind" fn(fetch: *mut emscripten_fetch_t)>,
    onreadystatechange: Option<unsafe extern "C-unwind" fn(fetch: *mut emscripten_fetch_t)>,

    /// EMSCRIPTEN_FETCH_* attributes
    attributes: u32,

    /// Specifies the amount of time the request can take before failing due to a
    /// timeout.
    timeoutMSecs: u32,

    /// Indicates whether cross-site access control requests should be made using
    /// credentials.
    withCredentials: bool,

    /// Specifies the destination path in IndexedDB where to store the downloaded
    /// content body. If this is empty, the transfer is not stored to IndexedDB at
    /// all.  Note that this struct does not contain space to hold this string, it
    /// only carries a pointer.
    /// Calling emscripten_fetch() will make an internal copy of this string.
    destinationPath: *const c_char,

    /// Specifies the authentication username to use for the request, if necessary.
    /// Note that this struct does not contain space to hold this string, it only
    /// carries a pointer.
    /// Calling emscripten_fetch() will make an internal copy of this string.
    userName: *const c_char,

    /// Specifies the authentication username to use for the request, if necessary.
    /// Note that this struct does not contain space to hold this string, it only
    /// carries a pointer.
    /// Calling emscripten_fetch() will make an internal copy of this string.
    password: *const c_char,

    /// Points to an array of strings to pass custom headers to the request. This
    /// array takes the form
    /// {"key1", "value1", "key2", "value2", "key3", "value3", ..., 0 }; Note
    /// especially that the array needs to be terminated with a null pointer.
    requestHeaders: *const *const c_char,

    /// Pass a custom MIME type here to force the browser to treat the received
    /// data with the given type.
    overriddenMimeType: *const c_char,

    /// If non-zero, specifies a pointer to the data that is to be passed as the
    /// body (payload) of the request that is being performed. Leave as zero if no
    /// request body needs to be sent.  The memory pointed to by this field is
    /// provided by the user, and needs to be valid throughout the duration of the
    /// fetch operation. If passing a non-zero pointer into this field, make sure
    /// to implement *both* the onsuccess and onerror handlers to be notified when
    /// the fetch finishes to know when this memory block can be freed. Do not pass
    /// a pointer to memory on the stack or other temporary area here.
    requestData: *const c_char,

    /// Specifies the length of the buffer pointed by 'requestData'. Leave as 0 if
    /// no request body needs to be sent.
    requestDataSize: usize,

    // Extra data, just in case this structure grows in size in the future.
    __extra: [*const c_void; 32],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct emscripten_fetch_t {
    /// Unique identifier for this fetch in progress.
    id: u32,

    /// Custom data that can be tagged along the process.
    userData: *mut c_void,

    /// The remote URL set in the original request.
    url: *const c_char,

    /// In onsuccess() handler:
    ///   - If the EMSCRIPTEN_FETCH_LOAD_TO_MEMORY attribute was specified for the
    ///     transfer, this points to the body of the downloaded data. Otherwise
    ///     this will be null.
    /// In onprogress() handler:
    ///   - If the EMSCRIPTEN_FETCH_STREAM_DATA attribute was specified for the
    ///     transfer, this points to a partial chunk of bytes related to the
    ///     transfer. Otherwise this will be null.
    /// The data buffer provided here has identical lifetime with the
    /// emscripten_fetch_t object itself, and is freed by calling
    /// emscripten_fetch_close() on the emscripten_fetch_t pointer.
    data: *const c_char,

    /// Specifies the length of the above data block in bytes. When the download
    /// finishes, this field will be valid even if EMSCRIPTEN_FETCH_LOAD_TO_MEMORY
    /// was not specified.
    numBytes: u64,

    /// If EMSCRIPTEN_FETCH_STREAM_DATA is being performed, this indicates the byte
    /// offset from the start of the stream that the data block specifies. (for
    /// onprogress() streaming XHR transfer, the number of bytes downloaded so far
    /// before this chunk)
    dataOffset: u64,

    /// Specifies the total number of bytes that the response body will be.
    /// Note: This field may be zero, if the server does not report the
    /// Content-Length field.
    totalBytes: u64,

    /// Specifies the readyState of the XHR request:
    /// 0: UNSENT: request not sent yet
    /// 1: OPENED: emscripten_fetch has been called.
    /// 2: HEADERS_RECEIVED: emscripten_fetch has been called, and headers and
    ///    status are available.
    /// 3: LOADING: download in progress.
    /// 4: DONE: download finished.
    /// See https://developer.mozilla.org/en-US/docs/Web/API/XMLHttpRequest/readyState
    readyState: c_ushort,

    /// Specifies the status code of the response.
    status: c_ushort,

    /// Specifies a human-readable form of the status code.
    statusText: [c_char; 64usize],

    /// For internal use only.
    __attributes: emscripten_fetch_attr_t,

    /// The response URL set by the fetch. It will be null until HEADERS_RECEIVED
    /// readyState in async, or until completion in sync.
    responseUrl: *const c_char,
}

#[link(name = "fetch")]
unsafe extern "C-unwind" {
    // Clears the fields of an emscripten_fetch_attr_t structure to their default
    // values in a future-compatible manner.
    fn emscripten_fetch_attr_init(fetch_attr: *mut emscripten_fetch_attr_t);

    // Initiates a new Emscripten fetch operation, which downloads data from the
    // given URL or from IndexedDB database.
    fn emscripten_fetch(
        fetch_attr: *mut emscripten_fetch_attr_t,
        url: *const c_char,
    ) -> *mut emscripten_fetch_t;

    #[deprecated]
    fn emscripten_fetch_wait(fetch: *mut emscripten_fetch_t, timeoutMSecs: f64) -> c_int;

    // Closes a finished or an executing fetch operation and frees up all memory. If
    // the fetch operation was still executing, the onerror() handler will be called
    // in the calling thread before this function returns.
    fn emscripten_fetch_close(fetch: *mut emscripten_fetch_t) -> c_int;

    // Gets the size (in bytes) of the response headers as plain text.
    // This must be called on the same thread as the fetch originated on.
    // Note that this will return 0 if readyState < HEADERS_RECEIVED.
    fn emscripten_fetch_get_response_headers_length(fetch: *mut emscripten_fetch_t) -> usize;

    // Gets the response headers as plain text. dstSizeBytes should be
    // headers_length + 1 (for the null terminator).
    // This must be called on the same thread as the fetch originated on.
    fn emscripten_fetch_get_response_headers(
        fetch: *mut emscripten_fetch_t,
        dst: *mut c_char,
        dstSizeBytes: usize,
    ) -> usize;

    // Converts the plain text headers into an array of strings. This array takes
    // the form {"key1", "value1", "key2", "value2", "key3", "value3", ..., 0 };
    // Note especially that the array is terminated with a null pointer.
    fn emscripten_fetch_unpack_response_headers(headersString: *const c_char) -> *mut *mut c_char;

    // This frees the memory used by the array of headers. Call this when finished
    // with the data returned by emscripten_fetch_unpack_response_headers.
    fn emscripten_fetch_free_unpacked_response_headers(unpackedHeaders: *mut *mut c_char);
}

pub fn test() {
    eprintln!("downloading");
    let mut builder = FetchBuilder::new("GET");
    builder.attr.attributes = FetchAttributes::empty().bits();
    builder.attr.onsuccess = Some(downloadSucceeded);
    builder.attr.onerror = Some(downloadFailed);
    // builder.attr.onprogress = Some(progress);
    let fetch = builder.build(c"http://example.com");
    eprintln!("sent");
}

extern "C-unwind" fn downloadSucceeded(fetch: *mut emscripten_fetch_t) {
    eprintln!("downloadSucceeded");
    let fetch2 = unsafe { &*fetch };
    println!(
        "Finished downloading {} bytes from URL {:?}",
        fetch2.numBytes,
        unsafe { CStr::from_ptr(fetch2.url) }
    );
    let bytes = unsafe { slice::from_raw_parts(fetch2.data.cast::<u8>(), fetch2.numBytes as _) };
    println!("{}", str::from_utf8(bytes).unwrap());
    unsafe { emscripten_fetch_close(fetch) };
}

extern "C-unwind" fn downloadFailed(fetch: *mut emscripten_fetch_t) {
    eprintln!("downloadFailed");
    let fetch2 = unsafe { &*fetch };
    println!(
        "Downloading {:?} failed, HTTP failure status code: {}",
        unsafe { CStr::from_ptr(fetch2.url) },
        fetch2.status
    );
    unsafe { emscripten_fetch_close(fetch) };
}
