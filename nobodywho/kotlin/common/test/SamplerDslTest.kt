package ai.nobodywho

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SamplerDslTest {

    @Test fun `shift steps are not silently dropped`() {
        val json = buildSampler { topK(40); temperature(0.8f); dist() }.toJson()
        val steps = JSONObject(json).getJSONArray("steps")
        assertEquals("expected 2 shift steps in resulting SamplerConfig, got: $json", 2, steps.length())
    }

    @Test fun `default terminal is dist`() {
        assertNotNull(buildSampler { topK(40); temperature(0.8f) })
    }

    @Test fun `explicit dist`() {
        assertNotNull(buildSampler { dist() })
    }

    @Test fun `explicit greedy`() {
        assertNotNull(buildSampler { greedy() })
    }

    @Test fun `explicit mirostatV1`() {
        assertNotNull(buildSampler { mirostatV1() })
    }

    @Test fun `explicit mirostatV2`() {
        assertNotNull(buildSampler { mirostatV2() })
    }

    @Test fun `all shift steps with dist`() {
        assertNotNull(buildSampler {
            topK(40); topP(0.9f); minP(0.05f); temperature(0.8f)
            typicalP(0.95f); xtc(0.1f, 0.5f); grammar("root ::= \"hi\"")
            dry(); penalties(); dist()
        })
    }

    @Test(expected = IllegalStateException::class)
    fun `double terminal throws`() {
        buildSampler { dist(); greedy() }
    }
}
