package ooo.nobodywho.nobodywho_testapp

import android.os.Bundle
import android.system.Os
import io.flutter.embedding.android.FlutterActivity

class MainActivity : FlutterActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        Os.setenv("GGML_OPENCL_ADRENO_XMEM_GEMM", "0", true)
        super.onCreate(savedInstanceState)
    }
}
