using UnityEngine;
using System.Collections;
using UnityEngine.Events;
using System;

namespace NobodyWho {
    public class Chat : MonoBehaviour {
        private IntPtr _workerContext;
        public Model model;
        public string systemPrompt = "You are a helpful assistant.";
        public UnityEvent<string> onToken = new UnityEvent<string>();
        public UnityEvent<string> onComplete = new UnityEvent<string>();

        private void OnToken(string token) => onToken.Invoke(token);
        private void OnComplete(string response) => onComplete.Invoke(response);
        private void OnError(string error) => Debug.LogError($"LLM Error: {error}");

        void Start() {
            // Create worker once using the Model component
            _workerContext = NativeBindings.create_chat_worker(model.GetModel(), systemPrompt);
        }

        void Update() {
            // Poll for responses
            NativeBindings.poll_responses(
                _workerContext,
                OnToken,
                OnComplete,
                OnError
            );
        }

        public void say(string prompt) {
            NativeBindings.send_prompt(_workerContext, prompt);
        }
    }
}