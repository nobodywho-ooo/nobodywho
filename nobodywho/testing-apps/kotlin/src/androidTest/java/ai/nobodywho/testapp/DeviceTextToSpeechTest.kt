package ai.nobodywho.testapp

import ai.nobodywho.TextToSpeech
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Regression test for https://github.com/nobodywho-ooo/nobodywho/issues/820.
 */
@RunWith(AndroidJUnit4::class)
class DeviceTextToSpeechTest {

    @Test
    fun textToSpeechLoadsAndSynthesizes() = runBlocking {
        val tts: TextToSpeech = TextToSpeech.load(
            source = "hf://NobodyWho/Kokoro-82M",
            voice = "bf_emma",
            language = "en-gb",
        )
        val wav: ByteArray = tts.synthesize("Hello from NobodyWho.")
        // 44-byte WAV header plus at least some samples.
        assertTrue("Expected WAV data, got ${wav.size} bytes", wav.size > 44)
        assertEquals("RIFF", String(wav, 0, 4, Charsets.US_ASCII))
        assertEquals("WAVE", String(wav, 8, 4, Charsets.US_ASCII))
    }
}
