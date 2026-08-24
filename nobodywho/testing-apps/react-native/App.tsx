/**
 * Device-test host app for the NobodyWho React Native binding.
 *
 * Runs the same three checks as the Kotlin and Flutter device tests —
 * completion, streaming and tool calling — and renders PASS or FAIL:<reason>.
 * The assertions live here, in JS, because this is where the library's own API
 * is reachable; the instrumentation test in android/app/src/androidTest only
 * launches the app and waits for that outcome via UI Automator.
 */
import React, {useEffect, useState} from 'react';
import {SafeAreaView, StyleSheet, Text} from 'react-native';
import {Chat, Tool} from 'react-native-nobodywho';

/**
 * Downloaded on-device into the app's own storage on first run — no
 * permissions and no shared storage involved.
 */
const MODEL_URL =
  'hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf';

const ping = () => 'pong';

async function runChecks(): Promise<void> {
  // Ask for the GPU like a real app would. Android has no GPU backend yet, so
  // this falls back to CPU today and starts exercising the GPU path
  // automatically once one lands.
  const chat = await Chat.fromPath({
    modelPath: MODEL_URL,
    systemPrompt: 'Reply with one word only.',
    templateVariables: {enable_thinking: false},
    useGpu: true,
  });

  // Completion
  const response = await chat.ask('Say hello').completed();
  if (!response) {
    throw new Error('completion was empty');
  }

  // Streaming
  await chat.resetContext({systemPrompt: 'Reply briefly.'});
  let tokenCount = 0;
  for await (const _token of chat.ask('Say hi')) {
    tokenCount++;
  }
  if (tokenCount === 0) {
    throw new Error('streaming yielded no tokens');
  }

  // Tool calling
  const pingTool = new Tool({
    name: 'ping',
    description: 'Ping the server',
    parameters: [],
    call: ping,
  });
  await chat.resetContext({
    systemPrompt: 'Use the ping tool now.',
    tools: [pingTool],
  });
  await chat.ask('Ping the server').completed();

  const history = await chat.getChatHistory();
  const toolMessage = history.find(m => m.role === 'tool');
  if (!toolMessage) {
    throw new Error('no tool response in chat history');
  }
  if (toolMessage.content !== 'pong') {
    throw new Error(`tool returned "${toolMessage.content}", expected "pong"`);
  }
}

export default function App(): React.JSX.Element {
  const [status, setStatus] = useState('RUNNING');

  useEffect(() => {
    runChecks()
      .then(() => setStatus('PASS'))
      .catch((e: unknown) => setStatus(`FAIL: ${String(e)}`));
  }, []);

  return (
    <SafeAreaView style={styles.container}>
      <Text testID="status" style={styles.status}>
        {status}
      </Text>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {flex: 1, alignItems: 'center', justifyContent: 'center'},
  status: {fontSize: 18, padding: 16, textAlign: 'center'},
});
