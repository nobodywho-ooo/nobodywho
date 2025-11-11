import 'package:flutter/material.dart';
import 'package:file_picker/file_picker.dart';
import 'package:nobodywho_flutter/nobodywho_flutter.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(const ChatApp());
}

class ChatApp extends StatelessWidget {
  const ChatApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'NobodyWho Chat Example',
      theme: ThemeData(
        primarySwatch: Colors.blue,
        useMaterial3: true,
      ),
      home: const ChatScreen(),
    );
  }
}

class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final List<Message> _messages = [];
  final TextEditingController _textController = TextEditingController();
  final ScrollController _scrollController = ScrollController();

  NobodyWhoModel? _model;
  NobodyWhoChat? _chat;
  bool _isModelLoaded = false;
  bool _isResponding = false;

  @override
  void initState() {
    super.initState();
  }

  Future<void> _showModelPicker() async {
    FilePickerResult? result = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['gguf'],
      dialogTitle: 'Select a GGUF model file',
    );

    if (result != null && result.files.single.path != null) {
      _loadModel(result.files.single.path!);
    }
  }

  void _loadModel(String modelPath) {
    setState(() {
      // Initialize the NobodyWho model
      _model = NobodyWhoModel(
        modelPath: modelPath,
        useGpu: false,
      );

      // Create a chat instance with the model
      _chat = NobodyWhoChat(
        model: _model!,
        systemPrompt: "You are a helpful assistant",
        contextSize: 2048,
        tools: [],
      );

      _isModelLoaded = true;

      // Add a system message showing the model is loaded
      _messages.add(Message(
        text: "Model loaded: ${modelPath.split('/').last}",
        isUser: false,
        isSystem: true,
      ));
    });
  }

  Future<void> _sendMessage(String text) async {
    if (text.trim().isEmpty || !_isModelLoaded || _isResponding) return;

    // Add user message
    setState(() {
      _messages.add(Message(text: text, isUser: true));
      _isResponding = true;
    });

    _textController.clear();
    _scrollToBottom();

    // Create a message for the assistant's response
    final assistantMessage = Message(text: "", isUser: false);
    setState(() {
      _messages.add(assistantMessage);
    });

    try {
      // Get the response stream from NobodyWho
      final responseStream = _chat!.say(message: text);

      // Stream the response token by token
      await for (final token in responseStream) {
        setState(() {
          assistantMessage.text += token;
        });
        _scrollToBottom();
      }
    } catch (e) {
      setState(() {
        assistantMessage.text = "Error: ${e.toString()}";
      });
    } finally {
      setState(() {
        _isResponding = false;
      });
    }
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 300),
          curve: Curves.easeOut,
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('NobodyWho Chat Example'),
        backgroundColor: Theme.of(context).colorScheme.inversePrimary,
        actions: [
          IconButton(
            icon: const Icon(Icons.folder_open),
            onPressed: _showModelPicker,
            tooltip: 'Load Model',
          ),
        ],
      ),
      body: _isModelLoaded
        ? Column(
            children: [
              // Chat messages area
              Expanded(
                child: ListView.builder(
                  controller: _scrollController,
                  padding: const EdgeInsets.all(8.0),
                  itemCount: _messages.length,
                  itemBuilder: (context, index) {
                    return _buildMessage(_messages[index]);
                  },
                ),
              ),

              // Input area
              Container(
                decoration: BoxDecoration(
                  color: Theme.of(context).cardColor,
                  boxShadow: [
                    BoxShadow(
                      offset: const Offset(0, -2),
                      blurRadius: 4,
                      color: Colors.black.withOpacity(0.1),
                    ),
                  ],
                ),
                child: Padding(
                  padding: const EdgeInsets.all(8.0),
                  child: Row(
                    children: [
                      Expanded(
                        child: TextField(
                          controller: _textController,
                          enabled: _isModelLoaded && !_isResponding,
                          decoration: InputDecoration(
                            hintText: _isResponding
                              ? 'Waiting for response...'
                              : 'Type a message...',
                            border: OutlineInputBorder(
                              borderRadius: BorderRadius.circular(25.0),
                            ),
                            contentPadding: const EdgeInsets.symmetric(
                              horizontal: 16.0,
                              vertical: 12.0,
                            ),
                          ),
                          onSubmitted: _sendMessage,
                        ),
                      ),
                      const SizedBox(width: 8.0),
                      IconButton(
                        icon: Icon(_isResponding ? Icons.stop : Icons.send),
                        onPressed: _isModelLoaded && !_isResponding
                          ? () => _sendMessage(_textController.text)
                          : null,
                      ),
                    ],
                  ),
                ),
              ),
            ],
          )
        : Center(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(
                  Icons.chat_bubble_outline,
                  size: 80,
                  color: Colors.grey.shade400,
                ),
                const SizedBox(height: 24),
                Text(
                  'Welcome to NobodyWho Chat',
                  style: Theme.of(context).textTheme.headlineSmall,
                ),
                const SizedBox(height: 16),
                Text(
                  'Select a GGUF model file to start chatting',
                  style: TextStyle(
                    color: Colors.grey.shade600,
                    fontSize: 16,
                  ),
                ),
                const SizedBox(height: 32),
                ElevatedButton.icon(
                  onPressed: _showModelPicker,
                  icon: const Icon(Icons.folder_open),
                  label: const Text('Select Model File'),
                  style: ElevatedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 24,
                      vertical: 16,
                    ),
                  ),
                ),
              ],
            ),
          ),
    );
  }

  Widget _buildMessage(Message message) {
    if (message.isSystem) {
      return Center(
        child: Container(
          margin: const EdgeInsets.symmetric(vertical: 8.0),
          padding: const EdgeInsets.symmetric(horizontal: 12.0, vertical: 6.0),
          decoration: BoxDecoration(
            color: Colors.grey.shade200,
            borderRadius: BorderRadius.circular(12.0),
          ),
          child: Text(
            message.text,
            style: TextStyle(
              color: Colors.grey.shade700,
              fontSize: 12.0,
              fontStyle: FontStyle.italic,
            ),
          ),
        ),
      );
    }

    return Align(
      alignment: message.isUser ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 4.0, horizontal: 8.0),
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.75,
        ),
        child: Card(
          color: message.isUser
            ? Theme.of(context).primaryColor.withOpacity(0.1)
            : Theme.of(context).cardColor,
          child: Padding(
            padding: const EdgeInsets.all(12.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  message.isUser ? 'You' : 'Assistant',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    fontSize: 12.0,
                    color: message.isUser
                      ? Theme.of(context).primaryColor
                      : Colors.grey.shade600,
                  ),
                ),
                const SizedBox(height: 4.0),
                Text(
                  message.text,
                  style: const TextStyle(fontSize: 14.0),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  @override
  void dispose() {
    _textController.dispose();
    _scrollController.dispose();
    super.dispose();
  }
}

// Simple message model
class Message {
  String text;
  final bool isUser;
  final bool isSystem;

  Message({
    required this.text,
    required this.isUser,
    this.isSystem = false,
  });
}
