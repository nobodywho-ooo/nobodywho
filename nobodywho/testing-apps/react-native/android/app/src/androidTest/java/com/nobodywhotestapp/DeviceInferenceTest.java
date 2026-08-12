package com.nobodywhotestapp;

import static androidx.test.platform.app.InstrumentationRegistry.getInstrumentation;
import static org.junit.Assert.fail;

import android.os.SystemClock;
import androidx.test.core.app.ActivityScenario;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.filters.LargeTest;
import androidx.test.uiautomator.By;
import androidx.test.uiautomator.UiDevice;
import androidx.test.uiautomator.UiObject2;
import org.junit.Test;
import org.junit.runner.RunWith;

/**
 * On-device smoke test, run on real hardware via Firebase Test Lab.
 *
 * The checks themselves live in App.tsx, which drives the NobodyWho JavaScript
 * API — completion, streaming and tool calling, mirroring the Kotlin and
 * Flutter device tests — and renders PASS or FAIL:&lt;reason&gt;. This test only
 * launches the app and waits for that outcome, which keeps it self-contained:
 * Detox is the idiomatic React Native e2e runner, but it drives tests from a
 * Node process on the host, and Firebase Test Lab only runs the APK pair.
 */
@RunWith(AndroidJUnit4.class)
@LargeTest
public class DeviceInferenceTest {

  /** Generous: the first run downloads the model before any inference starts. */
  private static final long TIMEOUT_MS = 20 * 60 * 1000L;

  private static final long POLL_MS = 2000L;

  @Test
  public void chatCompletesStreamsAndCallsTools() {
    ActivityScenario.launch(MainActivity.class);
    UiDevice device = UiDevice.getInstance(getInstrumentation());

    long deadline = SystemClock.uptimeMillis() + TIMEOUT_MS;
    while (SystemClock.uptimeMillis() < deadline) {
      if (device.hasObject(By.text("PASS"))) {
        return;
      }
      // Surface the library-level reason immediately rather than waiting out
      // the full timeout and reporting a bare "timed out".
      UiObject2 failure = device.findObject(By.textStartsWith("FAIL"));
      if (failure != null) {
        fail(failure.getText());
      }
      SystemClock.sleep(POLL_MS);
    }

    UiObject2 status = device.findObject(By.res("status"));
    fail(
        "app did not report a result within "
            + (TIMEOUT_MS / 60000)
            + " minutes; last status: "
            + (status != null ? status.getText() : "<not found>"));
  }
}
