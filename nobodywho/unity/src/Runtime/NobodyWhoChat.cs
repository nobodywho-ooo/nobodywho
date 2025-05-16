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

        private ChatWrapper wrapper = ChatWrapper.New();

        [TextArea(15, 20)]
        public string systemPrompt = "";

        [Header("Configuration")]
        public string stopWords = "";
        public uint contextLength = 4096;
        public bool use_grammar = false;
        // ^ TODO: can we be consistent about casing here?

        [TextArea(15, 20)]
        public string grammar;

        [Header("Events")]
        public UnityEvent<string> onToken = new UnityEvent<string>();
        public UnityEvent<string> onComplete = new UnityEvent<string>();

        public void StartWorker() {
            wrapper.StartWorker(model.ModelWrapperContext, contextLength, systemPrompt);
        }

        public void Say(string text)
        {
            wrapper.Say(text, use_grammar, grammar, stopWords);
        }

        public void Update() {
            var res = wrapper.PollResponse();
            switch (res.kind) {
                case PollKind.Nothing:
                    break;

                case PollKind.Token:
                    string token = Marshal.PtrToStringAnsi(res.ptr, (int)res.len);
                    onToken.Invoke(token);
                    break;

                case PollKind.Done:
                    string resp = Marshal.PtrToStringAnsi(res.ptr, (int)res.len);
                    onComplete.Invoke(resp);
                    break;
            }
        }
    }
}
