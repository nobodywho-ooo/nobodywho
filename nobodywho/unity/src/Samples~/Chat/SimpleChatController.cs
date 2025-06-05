using UnityEngine;
using UnityEngine.UIElements;
using NobodyWho;

public class SimpleChatController : MonoBehaviour
{
    [Header("References")]
    public Chat chat;
    public UIDocument uiDocument;
    private TextField messageInput;
    private Button sendButton;
    private Label chatText;
    private ScrollView chatHistory;
    
    private string conversationHistory = "Welcome! Start chatting...\n\n";
    
    void OnEnable()
    {
        Invoke(nameof(SetupUI), 0.1f);
    }
    
    void SetupUI()
    {
        var root = uiDocument.rootVisualElement;
        
        // Get UI elements with error checking
        messageInput = root.Q<TextField>("message-input");
        sendButton = root.Q<Button>("send-button");
        chatText = root.Q<Label>("chat-text");
        chatHistory = root.Q<ScrollView>("chat-history");
        
        // Set up event handlers
        if (sendButton != null)
            sendButton.clicked += SendMessage;
            
        if (messageInput != null)
        {
            messageInput.RegisterCallback<KeyDownEvent>(OnInputKeyDown);
            // Force focus on the input field
            messageInput.Focus();
        }
        
        // Start chat worker
        if (chat != null)
        {
            chat.StartWorker();
            chat.responseUpdated.AddListener(OnResponseUpdated);
            chat.responseFinished.AddListener(OnResponseFinished);
        }
        
        // Update initial display
        UpdateChatDisplay();
    }
    
    void OnDisable()
    {
        // Clean up event handlers
        if (sendButton != null)
            sendButton.clicked -= SendMessage;
            
        if (messageInput != null)
            messageInput.UnregisterCallback<KeyDownEvent>(OnInputKeyDown);
            
        if (chat != null)
        {
            chat.responseUpdated.RemoveListener(OnResponseUpdated);
            chat.responseFinished.RemoveListener(OnResponseFinished);
        }
    }
    
    void OnInputKeyDown(KeyDownEvent evt)
    {
        if (evt.keyCode == KeyCode.Return || evt.keyCode == KeyCode.KeypadEnter)
        {
            SendMessage();
            evt.StopPropagation();
        }
    }
    
    void SendMessage()
    {
        if (messageInput == null) return;
        
        string message = messageInput.value.Trim();
        if (!string.IsNullOrEmpty(message) && chat != null)
        {
            // Add user message to conversation
            conversationHistory += $"User: {message}\nAI: ";
            UpdateChatDisplay();
            
            // Send to chat AI
            chat.Say(message);
            
            // Clear input and refocus
            messageInput.value = "";
            messageInput.Focus();
        }
    }
    
    void OnResponseUpdated(string token)
    {
        conversationHistory += token;
        UpdateChatDisplay();
    }
    
    void OnResponseFinished(string fullResponse)
    {
        conversationHistory += "\n\n";
        UpdateChatDisplay();
    }
    
    void UpdateChatDisplay()
    {
        if (chatText != null)
        {
            chatText.text = conversationHistory;
            
            // Auto-scroll to bottom
            if (chatHistory != null)
            {
                chatHistory.schedule.Execute(() => {
                    chatHistory.scrollOffset = new Vector2(0, chatText.layout.height);
                }).ExecuteLater(10);
            }
        }
    }
} 