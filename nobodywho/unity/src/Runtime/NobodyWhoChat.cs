using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using UnityEngine;
using UnityEngine.Events;

namespace NobodyWho
{
    public class Chat : MonoBehaviour
    {
        public Model model;

        [TextArea(15, 20)]
        public string systemPrompt = "";

        [Header("Configuration")]
        public string stopWords = "";
        public int contextLength = 4096;
        public bool use_grammar = false;

        [TextArea(15, 20)]
        public string grammar;

        [Header("Events")]
        public UnityEvent<string> onToken = new UnityEvent<string>();
        public UnityEvent<string> onComplete = new UnityEvent<string>();

        private AwaitableCompletionSource<string> _completionSignal;
        private AwaitableCompletionSource<string> _tokenSignal;
        private IntPtr _workerContext;

        public void StartWorker()
        {
            // 0 is a null memory adress
            if (_workerContext != null && _workerContext != IntPtr.Zero)
            {
                NativeBindings.destroy_chat_worker(_workerContext);
            }
            try
            {
                var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
                // Todo - check if there is a builtin setter and getter that atoconverts to and from a string/string-array
                var stopWordsString = "";
                if (stopWords.Length > 0)
                {
                    stopWordsString = string.Join(",", stopWords);
                }
                _workerContext = NativeBindings.create_chat_worker(
                    model.GetModel(),
                    systemPrompt,
                    stopWordsString,
                    contextLength,
                    use_grammar,
                    grammar,
                    errorBuffer
                );
                if (_workerContext == IntPtr.Zero)
                {
                    throw new NobodyWhoException("Failed to create worker context");
                }
                if (errorBuffer.Length > 0)
                {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }
            }
            catch (Exception e)
            {
                throw new NobodyWhoException(e.Message);
            }
        }

        void OnDestroy()
        {
            NativeBindings.destroy_chat_worker(_workerContext);
            GC.Collect(); // added to e4nsure that is imeidieatley collected, as we are already sending stuff out on the other side of the ffi boundarry.
        }

        // This deletes the old worker context and creates a new one with the new params, it also means that we lose the chat history
        public void ResetContext()
        {
            StartWorker();
        }

        public Awaitable<string> Say(string prompt)
        {
            try
            {
                var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
                _completionSignal = new AwaitableCompletionSource<string>();

                NativeBindings.send_prompt(_workerContext, prompt, errorBuffer);
                if (errorBuffer.Length > 0)
                {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }

                return _completionSignal.Awaitable;
            }
            catch (Exception e)
            {
                throw new NobodyWhoException(e.Message);
            }
        }

        public void Update() {
            string token = NativeBindings.PollToken(_workerContext);
            if (token != null) {
                // _tokenSignal.SetResult(token);
                onToken.Invoke(token);
            }

            string response = NativeBindings.PollResponse(_workerContext);
            if (response != null) {
                _completionSignal.SetResult(response);
                onComplete.Invoke(response);
            }

            // TODO: handle error better
            // string error = NativeBindings.PollError(_workerContext);
            // if (error != null) {
            //     _errorSignal.SetResult(error);
            //     onError.Invoke(error);
            // }
        }
    }
}
