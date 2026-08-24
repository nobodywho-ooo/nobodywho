package ooo.nobodywho.nobodywho_testapp;

import androidx.test.rule.ActivityTestRule;
import dev.flutter.plugins.integration_test.FlutterTestRunner;
import org.junit.Rule;
import org.junit.runner.RunWith;

/**
 * Instrumentation entry point that hands control to the Dart tests in
 * `integration_test/`. Firebase Test Lab runs this; FlutterTestRunner reports
 * each Dart test as an instrumentation result.
 */
// MainActivity, not FlutterActivity: only the app's own activity is declared
// launchable in the manifest. Pointing the rule at the framework base class
// makes launchActivity fail, after which the runner waits for results that
// never arrive and the whole instrumentation run hangs until it is killed.
@RunWith(FlutterTestRunner.class)
public class MainActivityTest {
  @Rule
  public ActivityTestRule<MainActivity> rule =
      new ActivityTestRule<>(MainActivity.class, true, false);
}
