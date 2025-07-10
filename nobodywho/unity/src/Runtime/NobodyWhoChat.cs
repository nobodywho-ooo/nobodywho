using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Events;

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

        public List<ToolCall> tools = new List<ToolCall>();
        public void StartWorker()
        {
            wrapper.StartWorker(model.modelWrapperContext, contextLength, systemPrompt);
        }

        public void Say(string text)
        {
            wrapper.Say(text, useGrammar, grammar, stopWords);
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
        public void AddTool(Delegate userDelegate, string description)
        {
            ToolCall toolCall = new ToolCall(userDelegate, description);
            wrapper.AddTool(toolCall.callback, toolCall.name, toolCall.description, toolCall.jsonSchema);
        }

        public void ClearTools() {
            wrapper.ClearTools();
        }

        public void SetHistory(History history)
        {
            var history_json = JsonUtility.ToJson(history);
            wrapper.SetChatHistory(history_json);
        }

        public async Task<History> GetHistory()
        {
            // This needs to be async as it might be blocked for quite if the llm is already generating a response
            return await Task.Run(() =>
            {
                var res = wrapper.GetChatHistory();
                if (res.len == 0)
                    return new History(new List<Message>());

                string msgs = Marshal.PtrToStringAnsi(res.ptr, (int)res.len);
                return JsonUtility.FromJson<History>("{\"messages\":" + msgs + "}");
            });
        }

        [Serializable]
        public class History
        {
            public List<Message> messages;
            public History(List<Message> messages)
            {
                this.messages = messages;
            }
        }

        [Serializable]
        public class Message
        {
            public string role; // TODO change to enum (user, system, toolkcall, toolresponse)
            public string content;
            public Message(string role, string content)
            {
                this.role = role;
                this.content = content;
            }
        }

    }
}
