using UnityEngine;
using System.Collections;
using UnityEngine.Events;
using System;
using System.Text;
namespace NobodyWho {

    public class Chat : MonoBehaviour {
        private IntPtr _workerContext;
        public Model model;
        public string systemPrompt;
        public string[] stopTokens;
        public int contextLength;

        public UnityEvent<string> onToken = new UnityEvent<string>();
        public UnityEvent<string> onComplete = new UnityEvent<string>();

        private void OnToken(string token) => onToken.Invoke(token);
        private void OnComplete(string response) => onComplete.Invoke(response);
        private void OnError(string error) => Debug.LogError($"LLM Error: {error}");

        void Start() {
            try {
               var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
               // Todo - check if there is a builtin setter and getter that atoconverts to and from a string/string-array
               var stopTokensString = "";
               if (stopTokens.Length > 0) {
                stopTokensString = string.Join(",", stopTokens);
               }
                _workerContext = NativeBindings.create_chat_worker(model.GetModel(), systemPrompt, stopTokensString, contextLength, errorBuffer);
                if (errorBuffer.Length > 0) {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }

        void Update() {
            // we should do nothin unless we have a worker context
            if (_workerContext == null) {
                return;
            }
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
                var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
                NativeBindings.send_prompt(_workerContext, prompt, errorBuffer);
                if (errorBuffer.Length > 0) {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }

        void OnDestroy() {
            NativeBindings.destroy_chat_worker(_workerContext);
        }
    }
}