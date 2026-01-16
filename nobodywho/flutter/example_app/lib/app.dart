import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:file_picker/file_picker.dart';

import 'models/app_state.dart';
import 'screens/chat_screen.dart';
import 'utils/sampler_utils.dart';
import 'widgets/sampler_builder_panel.dart';

class ShowcaseApp extends StatelessWidget {
  const ShowcaseApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'NobodyWho Showcase',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.deepPurple),
        useMaterial3: true,
      ),
      home: const MainScreen(),
    );
  }
}

class MainScreen extends StatelessWidget {
  const MainScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();

    // If chat is ready, show the chat screen
    if (appState.isReady) {
      return const ActiveChatScreen();
    }

    // Show loading screen while model is loading
    if (appState.isModelLoading) {
      return const ModelLoadingScreen();
    }

    // Otherwise show the setup wizard
    return const SetupWizard();
  }
}

/// Loading screen displayed while the model is being loaded.
class ModelLoadingScreen extends StatelessWidget {
  const ModelLoadingScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();

    return Scaffold(
      body: Center(
        child: Card(
          margin: const EdgeInsets.all(32),
          child: Padding(
            padding: const EdgeInsets.all(48),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const SizedBox(
                  width: 64,
                  height: 64,
                  child: CircularProgressIndicator(strokeWidth: 4),
                ),
                const SizedBox(height: 32),
                Text(
                  'Loading Model',
                  style: Theme.of(context).textTheme.headlineSmall,
                ),
                const SizedBox(height: 8),
                Text(
                  appState.modelName ?? 'Please wait...',
                  style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                        color: Colors.grey.shade600,
                      ),
                ),
                const SizedBox(height: 24),
                Text(
                  'This may take a moment depending on the model size.',
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                        color: Colors.grey.shade500,
                      ),
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 16),
                // Show configuration summary
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.surfaceContainerHighest,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _buildInfoRow(context, 'Tools', '${appState.selectedTools.length} selected'),
                      _buildInfoRow(context, 'Sampler', appState.samplerDescription),
                      _buildInfoRow(context, 'Context', '${appState.contextSize} tokens'),
                      if (appState.useGpu)
                        _buildInfoRow(context, 'GPU', 'Enabled'),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildInfoRow(BuildContext context, String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 70,
            child: Text(
              '$label:',
              style: const TextStyle(fontWeight: FontWeight.w500, fontSize: 12),
            ),
          ),
          Text(value, style: const TextStyle(fontSize: 12)),
        ],
      ),
    );
  }
}

/// Setup wizard with stepper for configuring the chat.
class SetupWizard extends StatefulWidget {
  const SetupWizard({super.key});

  @override
  State<SetupWizard> createState() => _SetupWizardState();
}

class _SetupWizardState extends State<SetupWizard> {
  int _currentStep = 0;

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();

    return Scaffold(
      appBar: AppBar(
        title: const Text('NobodyWho Setup'),
        backgroundColor: Theme.of(context).colorScheme.inversePrimary,
      ),
      body: Stepper(
        currentStep: _currentStep,
        onStepContinue: () => _onStepContinue(appState),
        onStepCancel: _onStepCancel,
        onStepTapped: (step) => _onStepTapped(step, appState),
        controlsBuilder: (context, details) =>
            _buildControls(context, details, appState),
        steps: [
          // Step 1: Model Selection
          Step(
            title: const Text('Select Model'),
            subtitle: appState.modelPath != null
                ? Text(appState.modelName ?? 'Selected')
                : null,
            content: const ModelSelectionStep(),
            isActive: _currentStep >= 0,
            state: _getStepState(0, appState.modelPath != null),
          ),
          // Step 2: Tool Selection
          Step(
            title: const Text('Choose Tools'),
            subtitle: Text('${appState.selectedTools.length} selected'),
            content: const ToolSelectionStep(),
            isActive: _currentStep >= 1,
            state: _getStepState(1, true), // Tools are optional
          ),
          // Step 3: Sampler Configuration
          Step(
            title: const Text('Configure Sampler'),
            subtitle: Text(appState.samplerDescription),
            content: const SamplerConfigStep(),
            isActive: _currentStep >= 2,
            state: _getStepState(2, true), // Sampler is optional
          ),
          // Step 4: Final Settings & Load
          Step(
            title: const Text('Final Settings'),
            subtitle: const Text('System prompt & load'),
            content: const FinalSettingsStep(),
            isActive: _currentStep >= 3,
            state: _currentStep > 3 ? StepState.complete : StepState.indexed,
          ),
        ],
      ),
    );
  }

  StepState _getStepState(int step, bool isComplete) {
    if (_currentStep > step) {
      return StepState.complete;
    } else if (_currentStep == step) {
      return StepState.editing;
    } else {
      return StepState.indexed;
    }
  }

  void _onStepContinue(AppState appState) {
    if (_currentStep == 0 && appState.modelPath == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Please select a model file first')),
      );
      return;
    }

    if (_currentStep < 3) {
      setState(() {
        _currentStep += 1;
      });
    } else {
      // Final step - load the model
      _loadModel(appState);
    }
  }

  void _onStepCancel() {
    if (_currentStep > 0) {
      setState(() {
        _currentStep -= 1;
      });
    }
  }

  void _onStepTapped(int step, AppState appState) {
    // Only allow going back, or forward if previous steps are complete
    if (step < _currentStep) {
      setState(() {
        _currentStep = step;
      });
    } else if (step == _currentStep + 1) {
      _onStepContinue(appState);
    }
  }

  Future<void> _loadModel(AppState appState) async {
    final success = await appState.loadModelAndCreateChat();
    if (!success && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Error: ${appState.loadError ?? "Unknown error"}'),
          backgroundColor: Colors.red,
        ),
      );
    }
  }

  Widget _buildControls(
    BuildContext context,
    ControlsDetails details,
    AppState appState,
  ) {
    final isLastStep = _currentStep == 3;

    return Padding(
      padding: const EdgeInsets.only(top: 16),
      child: Row(
        children: [
          if (_currentStep > 0)
            TextButton(
              onPressed: details.onStepCancel,
              child: const Text('Back'),
            ),
          const SizedBox(width: 8),
          if (isLastStep)
            FilledButton.icon(
              onPressed: appState.isModelLoading ? null : details.onStepContinue,
              icon: appState.isModelLoading
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.rocket_launch),
              label: Text(appState.isModelLoading ? 'Loading...' : 'Start Chat'),
            )
          else
            FilledButton(
              onPressed: details.onStepContinue,
              child: const Text('Continue'),
            ),
        ],
      ),
    );
  }
}

/// Step 1: Model Selection
class ModelSelectionStep extends StatelessWidget {
  const ModelSelectionStep({super.key});

  Future<void> _pickModel(BuildContext context) async {
    final appState = context.read<AppState>();

    FilePickerResult? result = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['gguf'],
      dialogTitle: 'Select a GGUF model file',
    );

    if (result != null && result.files.single.path != null) {
      appState.setModelPath(result.files.single.path!);
    }
  }

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Select a GGUF model file to use for chat.',
          style: TextStyle(fontSize: 14),
        ),
        const SizedBox(height: 16),
        if (appState.modelPath != null)
          Card(
            child: ListTile(
              leading: const Icon(Icons.check_circle, color: Colors.green),
              title: Text(appState.modelName ?? 'Model selected'),
              subtitle: Text(
                appState.modelPath!,
                overflow: TextOverflow.ellipsis,
              ),
              trailing: IconButton(
                icon: const Icon(Icons.close),
                onPressed: () => appState.setModelPath(''),
              ),
            ),
          )
        else
          OutlinedButton.icon(
            onPressed: () => _pickModel(context),
            icon: const Icon(Icons.folder_open),
            label: const Text('Choose Model File'),
          ),
        const SizedBox(height: 16),
        SwitchListTile(
          title: const Text('Use GPU Acceleration'),
          subtitle: const Text('Enable for faster inference if available'),
          value: appState.useGpu,
          onChanged: (value) => appState.setUseGpu(value),
        ),
      ],
    );
  }
}

/// Step 2: Tool Selection with category grouping.
class ToolSelectionStep extends StatelessWidget {
  const ToolSelectionStep({super.key});

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Select which tools the model can use during chat.',
          style: TextStyle(fontSize: 14),
        ),
        const SizedBox(height: 8),
        Row(
          children: [
            TextButton(
              onPressed: () => appState.selectAllTools(),
              child: const Text('Select All'),
            ),
            TextButton(
              onPressed: () => appState.clearToolSelection(),
              child: const Text('Clear All'),
            ),
            const SizedBox(width: 8),
            Chip(
              label: Text('${appState.selectedTools.length} selected'),
              visualDensity: VisualDensity.compact,
            ),
          ],
        ),
        const SizedBox(height: 8),
        // Category-based tool selection
        ...ToolCategory.values.map((category) => _CategoryToolSection(
              category: category,
              appState: appState,
            )),
      ],
    );
  }
}

/// Expandable section for a tool category.
class _CategoryToolSection extends StatelessWidget {
  final ToolCategory category;
  final AppState appState;

  const _CategoryToolSection({
    required this.category,
    required this.appState,
  });

  @override
  Widget build(BuildContext context) {
    final categoryTools = AvailableTool.values
        .where((t) => t.category == category)
        .toList();
    final selectedCount = categoryTools
        .where((t) => appState.isToolSelected(t))
        .length;
    final isFullySelected = appState.isCategoryFullySelected(category);
    final isPartiallySelected = appState.isCategoryPartiallySelected(category);

    return Card(
      margin: const EdgeInsets.only(bottom: 8),
      child: ExpansionTile(
        leading: Checkbox(
          value: isFullySelected ? true : (isPartiallySelected ? null : false),
          tristate: true,
          onChanged: (_) {
            if (isFullySelected) {
              appState.deselectCategory(category);
            } else {
              appState.selectCategory(category);
            }
          },
        ),
        title: Text(category.label),
        subtitle: Text('$selectedCount/${categoryTools.length} selected'),
        children: categoryTools.map((tool) => CheckboxListTile(
              title: Text(tool.label),
              subtitle: Text(tool.description),
              value: appState.isToolSelected(tool),
              onChanged: (_) => appState.toggleTool(tool),
              dense: true,
              controlAffinity: ListTileControlAffinity.leading,
            )).toList(),
      ),
    );
  }
}

/// Step 3: Sampler Configuration
class SamplerConfigStep extends StatefulWidget {
  const SamplerConfigStep({super.key});

  @override
  State<SamplerConfigStep> createState() => _SamplerConfigStepState();
}

class _SamplerConfigStepState extends State<SamplerConfigStep> {
  SamplerPreset? _selectedPreset = SamplerPreset.defaultSampler;
  bool _useCustom = false;
  final SamplerBuilderState _builderState = SamplerBuilderState();

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Configure how the model generates text.',
          style: TextStyle(fontSize: 14),
        ),
        const SizedBox(height: 16),

        // Preset selection
        const Text('Quick Presets:', style: TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 8),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            ...SamplerPreset.values.map((preset) => ChoiceChip(
                  label: Text(preset.label),
                  selected: !_useCustom && _selectedPreset == preset,
                  onSelected: (selected) {
                    if (selected) {
                      setState(() {
                        _selectedPreset = preset;
                        _useCustom = false;
                      });
                      appState.setSamplerConfig(
                        getSamplerFromPreset(preset),
                        preset.label,
                      );
                    }
                  },
                )),
            ChoiceChip(
              label: const Text('Custom'),
              selected: _useCustom,
              onSelected: (selected) {
                setState(() {
                  _useCustom = selected;
                });
                if (selected) {
                  appState.setSamplerConfig(
                    _builderState.build(),
                    'Custom: ${_builderState.describe()}',
                  );
                }
              },
            ),
          ],
        ),

        if (_selectedPreset != null && !_useCustom) ...[
          const SizedBox(height: 8),
          Text(
            _selectedPreset!.description,
            style: TextStyle(
              fontSize: 12,
              color: Colors.grey.shade600,
              fontStyle: FontStyle.italic,
            ),
          ),
        ],

        // Custom builder (collapsed by default)
        if (_useCustom) ...[
          const SizedBox(height: 16),
          SamplerBuilderPanel(
            state: _builderState,
            onStateChanged: () {
              setState(() {});
              final appState = context.read<AppState>();
              appState.setSamplerConfig(
                _builderState.build(),
                'Custom: ${_builderState.describe()}',
              );
            },
          ),
        ],
      ],
    );
  }
}

/// Step 4: Final Settings
class FinalSettingsStep extends StatefulWidget {
  const FinalSettingsStep({super.key});

  @override
  State<FinalSettingsStep> createState() => _FinalSettingsStepState();
}

class _FinalSettingsStepState extends State<FinalSettingsStep> {
  late TextEditingController _promptController;

  @override
  void initState() {
    super.initState();
    final appState = context.read<AppState>();
    _promptController = TextEditingController(text: appState.systemPrompt);
  }

  @override
  void dispose() {
    _promptController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Configure final settings before starting.',
          style: TextStyle(fontSize: 14),
        ),
        const SizedBox(height: 16),

        // System prompt
        TextField(
          controller: _promptController,
          decoration: const InputDecoration(
            labelText: 'System Prompt',
            border: OutlineInputBorder(),
            hintText: 'Instructions for the model...',
          ),
          maxLines: 3,
          onChanged: (value) => appState.setSystemPrompt(value),
        ),
        const SizedBox(height: 16),

        // Context size
        Row(
          children: [
            const Text('Context Size: '),
            Expanded(
              child: Slider(
                value: appState.contextSize.toDouble(),
                min: 512,
                max: 8192,
                divisions: 15,
                label: appState.contextSize.toString(),
                onChanged: (value) => appState.setContextSize(value.round()),
              ),
            ),
            SizedBox(
              width: 60,
              child: Text('${appState.contextSize}'),
            ),
          ],
        ),

        // Allow thinking
        SwitchListTile(
          title: const Text('Allow Thinking'),
          subtitle: const Text('Enable for reasoning models'),
          value: appState.allowThinking,
          onChanged: (value) => appState.setAllowThinking(value),
        ),

        const SizedBox(height: 16),

        // Summary
        Card(
          color: Theme.of(context).colorScheme.primaryContainer,
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Configuration Summary',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    color: Theme.of(context).colorScheme.onPrimaryContainer,
                  ),
                ),
                const SizedBox(height: 8),
                Text('Model: ${appState.modelName ?? "None"}'),
                Text('Tools: ${appState.selectedTools.length} selected'),
                Text('Sampler: ${appState.samplerDescription}'),
                Text('Context: ${appState.contextSize} tokens'),
              ],
            ),
          ),
        ),

        if (appState.loadError != null) ...[
          const SizedBox(height: 16),
          Card(
            color: Colors.red.shade50,
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Row(
                children: [
                  Icon(Icons.error, color: Colors.red.shade700),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      appState.loadError!,
                      style: TextStyle(color: Colors.red.shade700),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ],
    );
  }
}

/// Active chat screen shown after setup is complete.
class ActiveChatScreen extends StatelessWidget {
  const ActiveChatScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final appState = context.watch<AppState>();

    return Scaffold(
      appBar: AppBar(
        title: Text(appState.modelName ?? 'Chat'),
        backgroundColor: Theme.of(context).colorScheme.inversePrimary,
        actions: [
          // Show active tools count
          if (appState.selectedTools.isNotEmpty)
            Chip(
              avatar: const Icon(Icons.build, size: 16),
              label: Text('${appState.selectedTools.length} tools'),
              visualDensity: VisualDensity.compact,
            ),
          const SizedBox(width: 8),
          // Reset button
          IconButton(
            icon: const Icon(Icons.restart_alt),
            tooltip: 'Start Over',
            onPressed: () {
              showDialog(
                context: context,
                builder: (context) => AlertDialog(
                  title: const Text('Start Over?'),
                  content: const Text(
                    'This will reset all settings and return to the setup wizard.',
                  ),
                  actions: [
                    TextButton(
                      onPressed: () => Navigator.pop(context),
                      child: const Text('Cancel'),
                    ),
                    FilledButton(
                      onPressed: () {
                        Navigator.pop(context);
                        context.read<AppState>().reset();
                      },
                      child: const Text('Reset'),
                    ),
                  ],
                ),
              );
            },
          ),
        ],
      ),
      body: const ChatScreen(),
    );
  }
}
