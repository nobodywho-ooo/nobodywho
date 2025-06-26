using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using UnityEngine;
using UnityEngine.Events;
using System.Linq;

namespace NobodyWho
{
    public class Chat : MonoBehaviour
    {
        public Model model;

        ChatWrapper wrapper = ChatWrapper.New();

        [TextArea(15, 20)]
        public string systemPrompt = "";

        [Header("Configuration")]
        public string stopWords = "";
        public uint contextLength = 4096;
        public bool useGrammar = false;

        [TextArea(15, 20)]
        public string grammar;

        [Header("Events")]
        public UnityEvent<string> responseUpdated = new UnityEvent<string>();
        public UnityEvent<string> responseFinished = new UnityEvent<string>();


        // Prevent garbage collection of tools
        private List<Delegate> _activeDelegates = new List<Delegate>();
        public void StartWorker()
        {
            wrapper.StartWorker(model.modelWrapperContext, contextLength, systemPrompt);
        }

        public void Say(string text)
        {
            wrapper.Say(text, useGrammar, grammar, stopWords);
        }

        public void AddTool(Delegate method, string description)
        {
            var info = method.Method;
            var parameters = info.GetParameters();

            string[] paramNames = parameters.Select(p => p.Name).ToArray();
            string[] paramTypes = parameters.Select(p => p.ParameterType.Name).ToArray();

            IntPtr toolcall = Marshal.GetFunctionPointerForDelegate(method);

            _activeDelegates.Add(method);

            Debug.Log($"Adding tool: {description}, {string.Join(", ", paramNames)}, {string.Join(", ", paramTypes)}");
            // wrapper.AddTool(toolcall, description, paramNames, paramTypes);
        }

        public void ResetContext()
        {
            wrapper.ResetContext(systemPrompt);
        }

        public void Stop()
        {
            wrapper.Stop();
        }

        public void Update()
        {
            var res = wrapper.PollResponse();
            switch (res.kind)
            {
                case PollResponseKind.Nothing:
                    break;

                case PollResponseKind.Token:
                    string token = Marshal.PtrToStringAnsi(res.ptr, (int)res.len);
                    responseUpdated.Invoke(token);
                    break;

                case PollResponseKind.Done:
                    string resp = Marshal.PtrToStringAnsi(res.ptr, (int)res.len);
                    responseFinished.Invoke(resp);
                    break;
            }
        }

        private void OnDestroy()
        {
            if (wrapper != null)
            {
                wrapper.Dispose();
                wrapper = null;
            }
        }

        public string GetResponseBlocking()
        {
            // this is only really used in tests.
            // it blocks forever, or until a finished response is emitted
            while (true)
            {
                var res = wrapper.PollResponse();
                switch (res.kind)
                {
                    case PollResponseKind.Done:
                        return Marshal.PtrToStringAnsi(res.ptr, (int)res.len);
                }
                System.Threading.Thread.Sleep(10);
            }
        }
    }
}
