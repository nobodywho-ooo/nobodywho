using UnityEngine;
using System.Collections;
using UnityEngine.Events;
using System;

namespace NobodyWho {

    public class Chat : MonoBehaviour {
        private IntPtr _workerContext;
        public Model model;
        public string systemPrompt;
        public UnityEvent<string> onToken = new UnityEvent<string>();
        public UnityEvent<string> onComplete = new UnityEvent<string>();

        private void OnToken(string token) => onToken.Invoke(token);
        private void OnComplete(string response) => onComplete.Invoke(response);
        private void OnError(string error) => Debug.LogError($"LLM Error: {error}");

        void Start() {
            try {
                var errorBuffer = new StringBuilder(256);
                _workerContext = NativeBindings.create_chat_worker(model.GetModel(), systemPrompt);
                if (errorBuffer.Length > 0) {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }

        void Update() {
            try {
                NativeBindings.poll_responses(
                    _workerContext,
                    OnToken,
                    OnComplete,
                    OnError
                );
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }

        public void say(string prompt) {
            try {
                var errorBuffer = new StringBuilder(256);
                NativeBindings.send_prompt(_workerContext, prompt, errorBuffer);
                if (errorBuffer.Length > 0) {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
            
        }
    }
}