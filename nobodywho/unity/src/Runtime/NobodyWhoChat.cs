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

        private string chatId;

        // Static callbacks to prevent garbage collection
        private static NativeBindings.ChatTokenCallback _tokenCallback = OnTokenCallback;
        private static NativeBindings.ChatCompletionCallback _completionCallback =
            OnCompletionCallback;
        private static NativeBindings.ChatErrorCallback _errorCallback = OnErrorCallback;

        void Start()
        {
            try
            {
                chatId = Guid.NewGuid().ToString();
                CrossAppDomainSingleton<ChatManager>.Instance.RegisterChat(chatId, this);

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
                    chatId,
                    _tokenCallback,
                    _completionCallback,
                    _errorCallback,
                    errorBuffer
                );

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
            CrossAppDomainSingleton<ChatManager>.Instance.UnregisterChat(chatId);
            NativeBindings.destroy_chat_worker(_workerContext);
        }

        // This deletes the old worker context and creates a new one with the new params, it also means that we lose the chat history
        public void ResetContext()
        {
            NativeBindings.destroy_chat_worker(_workerContext);

            var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
            _workerContext = NativeBindings.create_chat_worker(
                model.GetModel(),
                systemPrompt,
                stopWords,
                contextLength,
                use_grammar,
                grammar,
                chatId,
                _tokenCallback,
                _completionCallback,
                _errorCallback,
                errorBuffer
            );

            if (errorBuffer.Length > 0)
            {
                throw new NobodyWhoException(errorBuffer.ToString());
            }
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

        // the AOT (ahead of time compile) is required for ios as their security model does not allow JIT. https://stackoverflow.com/questions/5054732/is-it-prohibited-using-of-jitjust-in-time-compiled-code-in-ios-app-for-appstor
        // the P/invoke marks this as a callback that can be called from native code to avoid it being optimized away.
        [AOT.MonoPInvokeCallback(typeof(NativeBindings.ChatTokenCallback))]
        private static void OnTokenCallback(IntPtr callerPtr, IntPtr tokenPtr)
        {
            string chatId = Marshal.PtrToStringUTF8(callerPtr);
            string token = Marshal.PtrToStringUTF8(tokenPtr);

            Chat instance = CrossAppDomainSingleton<ChatManager>.Instance.GetChat(chatId);
            if (instance != null)
            {
                instance.onToken.Invoke(token);
                instance._tokenSignal?.SetResult(token);
            }
        }

        [AOT.MonoPInvokeCallback(typeof(NativeBindings.ChatCompletionCallback))]
        private static void OnCompletionCallback(IntPtr callerPtr, IntPtr responsePtr)
        {
            string chatId = Marshal.PtrToStringUTF8(callerPtr);
            string response = Marshal.PtrToStringUTF8(responsePtr);

            Chat instance = CrossAppDomainSingleton<ChatManager>.Instance.GetChat(chatId);
            if (instance != null)
            {
                instance.onComplete.Invoke(response);
                instance._completionSignal?.SetResult(response);
            }
        }

        [AOT.MonoPInvokeCallback(typeof(NativeBindings.ChatErrorCallback))]
        private static void OnErrorCallback(IntPtr callerPtr, IntPtr errorPtr)
        {
            string chatId = Marshal.PtrToStringUTF8(callerPtr);
            string error = Marshal.PtrToStringUTF8(errorPtr);

            Chat instance = CrossAppDomainSingleton<ChatManager>.Instance.GetChat(chatId);
            if (instance != null)
            {
                Debug.LogError($"Error while generating response: {error}");
                instance.onComplete.Invoke(error);
                instance._completionSignal?.SetException(new NobodyWhoException(error));
            }
        }
    }

    /// <summary>
    /// The chat manager is required for us to wrap the Chat in a MarshalbyRefObject,
    /// if it is not wrapped we lose the ability to proxy the chatinstances to another appdomain.
    /// This is basically like adding Arc (which implies send) on a object in rust, except we
    /// are not atomically updating the reference count.
    ///
    /// TODO: A future improvement to this would be to make it completely threads safe:
    /// - Use a concurrent dictionary
    /// - Use a lock for each individual Weakref.
    /// </summary>
    public class ChatManager : MarshalByRefObject
    {
        private Dictionary<string, WeakReference> chatInstances =
            new Dictionary<string, WeakReference>();

        public void RegisterChat(string id, Chat chat)
        {
            chatInstances[id] = new WeakReference(chat);
        }

        // Maybe we should free this object if there are no more live refs to any Chatinstance.
        // But I do not believe the overhead of having this being a longlived object is that big.
        // Also I am not sure wheter this leaks through the appdomains life time or not.
        public void UnregisterChat(string id)
        {
            if (chatInstances.ContainsKey(id))
            {
                chatInstances.Remove(id);
            }
        }

        public Chat GetChat(string id)
        {
            if (chatInstances.TryGetValue(id, out WeakReference weakRef) && weakRef.IsAlive)
            {
                return weakRef.Target as Chat;
            }
            return null;
        }

        public override object InitializeLifetimeService()
        {
            return null; // Live forever
        }
    }
}

/// <summary>
/// A cross app domain singleton to manage calls invoked on different threads.
/// This is needed for us to invoke callbakcs on the object in another thread in rust.
///
/// I believe the overhead is rather small of this, but i am not entirely sure and have not tested this.
/// </summary>
public class CrossAppDomainSingleton<T> : MarshalByRefObject
    where T : MarshalByRefObject, new()
{
    private static T _instance;
    private static readonly object _lock = new object();

    public static T Instance
    {
        get
        {
            if (_instance == null)
            {
                lock (_lock)
                {
                    if (_instance == null)
                    {
                        _instance = new T();
                    }
                }
            }
            return _instance;
        }
    }

    // Override lifetime for remote proxy
    public override object InitializeLifetimeService()
    {
        return null; // Live forever
    }
}
