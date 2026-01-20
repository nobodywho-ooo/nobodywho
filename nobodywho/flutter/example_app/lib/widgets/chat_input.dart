import 'package:flutter/material.dart';

/// Text input field with send/stop button for chat.
class ChatInput extends StatelessWidget {
  final TextEditingController controller;
  final bool isResponding;
  final bool enabled;
  final VoidCallback onSend;
  final VoidCallback onStop;

  const ChatInput({
    super.key,
    required this.controller,
    required this.isResponding,
    required this.enabled,
    required this.onSend,
    required this.onStop,
  });

  void _handleSubmit() {
    if (controller.text.trim().isNotEmpty && !isResponding) {
      onSend();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: Theme.of(context).cardColor,
        boxShadow: [
          BoxShadow(
            offset: const Offset(0, -2),
            blurRadius: 4,
            color: Colors.black.withAlpha(25),
          ),
        ],
      ),
      child: Padding(
        padding: const EdgeInsets.all(8.0),
        child: Row(
          children: [
            Expanded(
              child: TextField(
                controller: controller,
                enabled: enabled && !isResponding,
                decoration: InputDecoration(
                  hintText: isResponding
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
                onSubmitted: (_) => _handleSubmit(),
                textInputAction: TextInputAction.send,
              ),
            ),
            const SizedBox(width: 8.0),
            if (isResponding)
              IconButton.filled(
                icon: const Icon(Icons.stop),
                onPressed: onStop,
                tooltip: 'Stop generation',
                style: IconButton.styleFrom(
                  backgroundColor: Colors.red.shade400,
                ),
              )
            else
              ListenableBuilder(
                listenable: controller,
                builder: (context, child) {
                  return IconButton.filled(
                    icon: const Icon(Icons.send),
                    onPressed:
                        enabled && controller.text.trim().isNotEmpty ? onSend : null,
                    tooltip: 'Send message',
                  );
                },
              ),
          ],
        ),
      ),
    );
  }
}
