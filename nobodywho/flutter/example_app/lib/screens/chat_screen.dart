import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:nobodywho_flutter/nobodywho_flutter.dart' as nobodywho;

import '../models/app_state.dart';
import '../widgets/message_list.dart';
import '../widgets/chat_input.dart';

/// Chat screen that uses the pre-configured chat from AppState.
class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final List<nobodywho.Message> _messages = [];
  final TextEditingController _textController = TextEditingController();
  bool _isResponding = false;
  String? _streamingContent;

  @override
  void initState() {
    super.initState();
    _messages.add(nobodywho.Message.message(
      role: nobodywho.Role.system,
      content: 'Chat ready. Send a message to begin!',
    ));
  }

  Future<void> _sendMessage() async {
    final appState = context.read<AppState>();
    final chat = appState.chat;

    final text = _textController.text.trim();
    if (text.isEmpty || chat == null || _isResponding) return;

    setState(() {
      _messages.add(nobodywho.Message.message(
        role: nobodywho.Role.user,
        content: text,
      ));
      _isResponding = true;
      _streamingContent = '';
    });
    _textController.clear();

    try {
      final responseStream = chat.ask(text);

      await for (final token in responseStream) {
        if (!mounted) return;
        setState(() {
          _streamingContent = (_streamingContent ?? '') + token;
        });
      }

      // Streaming complete - fetch the actual chat history from the backend
      // This ensures we have the correct messages including any tool calls/responses
      if (mounted) {
        final history = await chat.getChatHistory();
        setState(() {
          // Keep the initial system message, replace the rest with actual history
          _messages.clear();
          _messages.add(nobodywho.Message.message(
            role: nobodywho.Role.system,
            content: 'Chat ready. Send a message to begin!',
          ));
          _messages.addAll(history);
          _streamingContent = null;
        });
      }
    } catch (e) {
      if (!mounted) return;
      setState(() {
        // On error, add an error message
        _messages.add(nobodywho.Message.message(
          role: nobodywho.Role.assistant,
          content: 'Error: ${e.toString()}',
        ));
        _streamingContent = null;
      });
    } finally {
      if (mounted) {
        setState(() {
          _isResponding = false;
        });
      }
    }
  }

  void _stopGeneration() {
    final appState = context.read<AppState>();
    appState.chat?.stopGeneration();
  }

  Future<void> _resetHistory() async {
    final appState = context.read<AppState>();
    final chat = appState.chat;
    if (chat == null) return;

    try {
      await chat.resetHistory();
      setState(() {
        _messages.clear();
        _messages.add(nobodywho.Message.message(
          role: nobodywho.Role.system,
          content: 'Chat history cleared.',
        ));
      });
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Error: ${e.toString()}')),
        );
      }
    }
  }

  void _showInfoSheet() {
    final appState = context.read<AppState>();

    showModalBottomSheet(
      context: context,
      builder: (context) => Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Chat Configuration',
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 16),
            _buildInfoRow('Model', appState.modelName ?? 'Unknown'),
            _buildInfoRow('Tools', '${appState.selectedTools.length} enabled'),
            _buildInfoRow('Sampler', appState.samplerDescription),
            _buildInfoRow('Context Size', '${appState.contextSize} tokens'),
            _buildInfoRow('Thinking Mode', appState.allowThinking ? 'Enabled' : 'Disabled'),
            const SizedBox(height: 8),
            const Divider(),
            const SizedBox(height: 8),
            Text(
              'System Prompt:',
              style: Theme.of(context).textTheme.titleSmall,
            ),
            const SizedBox(height: 4),
            Container(
              padding: const EdgeInsets.all(8),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Text(
                appState.systemPrompt,
                style: const TextStyle(fontSize: 12),
              ),
            ),
            const SizedBox(height: 16),
            OutlinedButton.icon(
              onPressed: () {
                Navigator.pop(context);
                _resetHistory();
              },
              icon: const Icon(Icons.delete_outline),
              label: const Text('Clear Chat History'),
            ),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }

  Widget _buildInfoRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 100,
            child: Text(
              '$label:',
              style: const TextStyle(fontWeight: FontWeight.bold),
            ),
          ),
          Expanded(child: Text(value)),
        ],
      ),
    );
  }

  @override
  void dispose() {
    _textController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();
    final chat = appState.chat;

    return Column(
      children: [
        // Config summary bar
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          color: Theme.of(context).colorScheme.surfaceContainerHighest,
          child: Row(
            children: [
              Icon(
                Icons.smart_toy,
                size: 16,
                color: Theme.of(context).colorScheme.primary,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  appState.samplerDescription,
                  style: const TextStyle(fontSize: 12),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (appState.allowThinking)
                Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: Chip(
                    label: const Text('Thinking', style: TextStyle(fontSize: 10)),
                    visualDensity: VisualDensity.compact,
                    padding: EdgeInsets.zero,
                  ),
                ),
              IconButton(
                icon: const Icon(Icons.info_outline, size: 20),
                onPressed: _showInfoSheet,
                tooltip: 'View Configuration',
              ),
            ],
          ),
        ),
        // Messages
        Expanded(
          child: MessageList(
            messages: _messages,
            streamingContent: _streamingContent,
          ),
        ),
        // Input
        ChatInput(
          controller: _textController,
          isResponding: _isResponding,
          enabled: chat != null,
          onSend: _sendMessage,
          onStop: _stopGeneration,
        ),
      ],
    );
  }
}
