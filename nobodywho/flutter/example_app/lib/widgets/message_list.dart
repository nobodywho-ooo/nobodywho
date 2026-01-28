import 'package:flutter/material.dart';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

import 'message_bubble.dart';

/// Scrollable list of chat messages with auto-scroll functionality.
class MessageList extends StatefulWidget {
  final List<nobodywho.Message> messages;
  final String? streamingContent;

  const MessageList({
    super.key,
    required this.messages,
    this.streamingContent,
  });

  @override
  State<MessageList> createState() => _MessageListState();
}

class _MessageListState extends State<MessageList> {
  final ScrollController _scrollController = ScrollController();

  @override
  void didUpdateWidget(MessageList oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Auto-scroll when new messages arrive or streaming content updates
    if (widget.messages.length > oldWidget.messages.length ||
        widget.streamingContent != oldWidget.streamingContent) {
      _scrollToBottom();
    }
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
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final hasStreamingContent =
        widget.streamingContent != null && widget.streamingContent!.isNotEmpty;
    final totalItems =
        widget.messages.length + (widget.streamingContent != null ? 1 : 0);

    if (widget.messages.isEmpty && !hasStreamingContent) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              Icons.chat_bubble_outline,
              size: 48,
              color: Colors.grey.shade400,
            ),
            const SizedBox(height: 16),
            Text(
              'Start a conversation',
              style: TextStyle(
                color: Colors.grey.shade600,
                fontSize: 16,
              ),
            ),
          ],
        ),
      );
    }

    return ListView.builder(
      controller: _scrollController,
      padding: const EdgeInsets.all(8.0),
      itemCount: totalItems,
      itemBuilder: (context, index) {
        if (index < widget.messages.length) {
          return MessageBubble(message: widget.messages[index]);
        } else {
          // This is the streaming message
          return StreamingMessageBubble(content: widget.streamingContent ?? '');
        }
      },
    );
  }
}
