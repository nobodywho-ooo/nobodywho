import 'package:flutter/material.dart';
import 'package:nobodywho_flutter/nobodywho_flutter.dart';
import 'package:file_picker/file_picker.dart';

Future<String> sparklify({required String thestring}) async {
  return "✨$thestring✨";
}

Future<void> main() async {
  await RustLib.init();
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'NobodyWho Chat',
      theme: ThemeData(
        primarySwatch: Colors.blue,
        useMaterial3: true,
      ),
      home: const ModelSelectionScreen(),
    );
  }
}

class ModelSelectionScreen extends StatefulWidget {
  const ModelSelectionScreen({super.key});

  @override
  State<ModelSelectionScreen> createState() => _ModelSelectionScreenState();
}

class _ModelSelectionScreenState extends State<ModelSelectionScreen> {
  String? _selectedModelPath;

  Future<void> _pickModelFile() async {
    print('_pickModelFile called');
    try {
      final FilePickerResult? result = await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['gguf'],
        dialogTitle: 'Select GGUF Model File',
      );

      print('File picker result: $result');

      if (result != null && result.files.single.path != null) {
        print('Selected file: ${result.files.single.path}');
        setState(() {
          _selectedModelPath = result.files.single.path;
        });
      } else {
        print('No file selected or path is null');
      }
    } catch (e) {
      print('Error picking file: $e');
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Select Model'),
        backgroundColor: Theme.of(context).colorScheme.inversePrimary,
      ),
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(
                Icons.smart_toy,
                size: 80,
                color: Colors.blue,
              ),
              const SizedBox(height: 32),
              const Text(
                'Select a GGUF model file to start chatting',
                style: TextStyle(fontSize: 18),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 32),
              if (_selectedModelPath != null) ...[
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: Colors.grey[100],
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: Colors.grey[300]!),
                  ),
                  child: Row(
                    children: [
                      const Icon(Icons.file_present, color: Colors.green),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          _selectedModelPath!.split('/').last,
                          style: const TextStyle(fontWeight: FontWeight.bold),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 16),
              ],
              ElevatedButton.icon(
                onPressed: _pickModelFile,
                icon: const Icon(Icons.folder_open),
                label: Text(_selectedModelPath == null ? 'Select Model File' : 'Change Model File'),
                style: ElevatedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
                ),
              ),
              if (_selectedModelPath != null) ...[
                const SizedBox(height: 16),
                ElevatedButton.icon(
                  onPressed: () {
                    Navigator.push(
                      context,
                      MaterialPageRoute(
                        builder: (context) => ChatScreen(modelPath: _selectedModelPath!),
                      ),
                    );
                  },
                  icon: const Icon(Icons.chat),
                  label: const Text('Start Chatting'),
                  style: ElevatedButton.styleFrom(
                    backgroundColor: Colors.blue,
                    foregroundColor: Colors.white,
                    padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class ChatScreen extends StatefulWidget {
  final String modelPath;

  const ChatScreen({super.key, required this.modelPath});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final TextEditingController _controller = TextEditingController();
  final List<ChatMessage> _messages = [];
  final ScrollController _scrollController = ScrollController();

  late NobodyWhoModel _model;
  late NobodyWhoChat _chat;
  bool _isInitialized = false;
  bool _isTyping = false;

  @override
  void initState() {
    super.initState();
    _initializeChat();
  }

  Future<void> _initializeChat() async {
    _model = NobodyWhoModel(
      modelPath: widget.modelPath,
      useGpu: true,
    );

    final tool = toolFromFunction(
      function: sparklify,
      name: "sparklify",
      description: "Applies the sparklify effect to a given string",
    );

    _chat = NobodyWhoChat(
      model: _model,
      systemPrompt: "You are a helpful assistant.",
      contextSize: 2048,
      tools: [tool],
    );

    setState(() {
      _isInitialized = true;
    });
  }

  Future<void> _sendMessage() async {
    final text = _controller.text.trim();
    if (text.isEmpty || !_isInitialized || _isTyping) return;

    _controller.clear();

    setState(() {
      _messages.add(ChatMessage(text: text, isUser: true));
      _messages.add(ChatMessage(text: "", isUser: false));
      _isTyping = true;
    });

    _scrollToBottom();

    try {
      final responseStream = await _chat.say(message: text);
      String fullResponse = "";

      await for (final token in responseStream) {
        fullResponse += token;
        setState(() {
          _messages[_messages.length - 1] = ChatMessage(
            text: fullResponse,
            isUser: false,
          );
        });
        _scrollToBottom();
      }
    } catch (e) {
      setState(() {
        _messages[_messages.length - 1] = ChatMessage(
          text: "Error: ${e.toString()}",
          isUser: false,
        );
      });
    }

    setState(() {
      _isTyping = false;
    });
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 200),
          curve: Curves.easeOut,
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('NobodyWho Chat'),
        backgroundColor: Theme.of(context).colorScheme.inversePrimary,
      ),
      body: Column(
        children: [
          if (!_isInitialized)
            const LinearProgressIndicator()
          else if (_isTyping)
            LinearProgressIndicator(
              backgroundColor: Colors.grey[300],
            ),
          Expanded(
            child: ListView.builder(
              controller: _scrollController,
              padding: const EdgeInsets.all(8),
              itemCount: _messages.length,
              itemBuilder: (context, index) {
                return MessageBubble(message: _messages[index]);
              },
            ),
          ),
          Container(
            padding: const EdgeInsets.all(8),
            decoration: BoxDecoration(
              color: Colors.grey[100],
              border: Border(
                top: BorderSide(color: Colors.grey[300]!),
              ),
            ),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _controller,
                    decoration: const InputDecoration(
                      hintText: 'Type a message...',
                      border: OutlineInputBorder(),
                      contentPadding: EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 8,
                      ),
                    ),
                    onSubmitted: (_) => _sendMessage(),
                    enabled: _isInitialized && !_isTyping,
                  ),
                ),
                const SizedBox(width: 8),
                IconButton(
                  onPressed: _isInitialized && !_isTyping ? _sendMessage : null,
                  icon: const Icon(Icons.send),
                  color: Theme.of(context).primaryColor,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  @override
  void dispose() {
    _controller.dispose();
    _scrollController.dispose();
    super.dispose();
  }
}

class ChatMessage {
  final String text;
  final bool isUser;

  ChatMessage({required this.text, required this.isUser});
}

class MessageBubble extends StatelessWidget {
  final ChatMessage message;

  const MessageBubble({super.key, required this.message});

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: message.isUser ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 4),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        decoration: BoxDecoration(
          color: message.isUser ? Colors.blue : Colors.grey[300],
          borderRadius: BorderRadius.circular(12),
        ),
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.75,
        ),
        child: Text(
          message.text,
          style: TextStyle(
            color: message.isUser ? Colors.white : Colors.black87,
          ),
        ),
      ),
    );
  }
}
