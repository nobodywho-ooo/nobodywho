import 'package:flutter/material.dart';

import '../utils/sampler_utils.dart';

/// Panel for building a custom sampler chain with all parameters exposed.
class SamplerBuilderPanel extends StatelessWidget {
  final SamplerBuilderState state;
  final VoidCallback onStateChanged;

  const SamplerBuilderPanel({
    super.key,
    required this.state,
    required this.onStateChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.tune, size: 20),
                const SizedBox(width: 8),
                Text(
                  'Custom Sampler Builder',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const Spacer(),
                TextButton.icon(
                  onPressed: () {
                    state.reset();
                    onStateChanged();
                  },
                  icon: const Icon(Icons.refresh, size: 16),
                  label: const Text('Reset'),
                ),
              ],
            ),
            const Divider(),

            // Temperature
            _SamplerStepSection(
              title: 'Temperature',
              tooltip: 'Controls randomness. 0 = deterministic, >1 = creative',
              enabled: state.useTemperature,
              onEnabledChanged: (v) {
                state.useTemperature = v;
                onStateChanged();
              },
              children: [
                _buildSliderRow(
                  context,
                  label: 'Value',
                  value: state.temperature,
                  min: 0.0,
                  max: 2.0,
                  divisions: 40,
                  onChanged: (v) {
                    state.temperature = v;
                    onStateChanged();
                  },
                ),
              ],
            ),

            // Top-K
            _SamplerStepSection(
              title: 'Top-K',
              tooltip: 'Keep only top K most probable tokens',
              enabled: state.useTopK,
              onEnabledChanged: (v) {
                state.useTopK = v;
                onStateChanged();
              },
              children: [
                _buildSliderRow(
                  context,
                  label: 'K',
                  value: state.topK.toDouble(),
                  min: 1,
                  max: 100,
                  divisions: 99,
                  displayValue: state.topK.toString(),
                  onChanged: (v) {
                    state.topK = v.round();
                    onStateChanged();
                  },
                ),
              ],
            ),

            // Top-P (nucleus)
            _SamplerStepSection(
              title: 'Top-P (Nucleus)',
              tooltip: 'Keep tokens summing to P probability',
              enabled: state.useTopP,
              onEnabledChanged: (v) {
                state.useTopP = v;
                onStateChanged();
              },
              children: [
                _buildSliderRow(
                  context,
                  label: 'P',
                  value: state.topP,
                  min: 0.0,
                  max: 1.0,
                  divisions: 20,
                  onChanged: (v) {
                    state.topP = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Min Keep',
                  value: state.topPMinKeep.toDouble(),
                  min: 1,
                  max: 10,
                  divisions: 9,
                  displayValue: state.topPMinKeep.toString(),
                  onChanged: (v) {
                    state.topPMinKeep = v.round();
                    onStateChanged();
                  },
                ),
              ],
            ),

            // Min-P
            _SamplerStepSection(
              title: 'Min-P',
              tooltip: 'Minimum relative probability threshold',
              enabled: state.useMinP,
              onEnabledChanged: (v) {
                state.useMinP = v;
                onStateChanged();
              },
              children: [
                _buildSliderRow(
                  context,
                  label: 'Min P',
                  value: state.minP,
                  min: 0.0,
                  max: 0.5,
                  divisions: 50,
                  onChanged: (v) {
                    state.minP = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Min Keep',
                  value: state.minPMinKeep.toDouble(),
                  min: 1,
                  max: 10,
                  divisions: 9,
                  displayValue: state.minPMinKeep.toString(),
                  onChanged: (v) {
                    state.minPMinKeep = v.round();
                    onStateChanged();
                  },
                ),
              ],
            ),

            // Typical-P
            _SamplerStepSection(
              title: 'Typical-P',
              tooltip: 'Keep tokens close to expected information content',
              enabled: state.useTypicalP,
              onEnabledChanged: (v) {
                state.useTypicalP = v;
                onStateChanged();
              },
              children: [
                _buildSliderRow(
                  context,
                  label: 'Typical P',
                  value: state.typicalP,
                  min: 0.0,
                  max: 1.0,
                  divisions: 20,
                  onChanged: (v) {
                    state.typicalP = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Min Keep',
                  value: state.typicalPMinKeep.toDouble(),
                  min: 1,
                  max: 10,
                  divisions: 9,
                  displayValue: state.typicalPMinKeep.toString(),
                  onChanged: (v) {
                    state.typicalPMinKeep = v.round();
                    onStateChanged();
                  },
                ),
              ],
            ),

            // XTC
            _SamplerStepSection(
              title: 'XTC (eXclude Top Choices)',
              tooltip: 'Probabilistically exclude high-probability tokens for diversity',
              enabled: state.useXtc,
              onEnabledChanged: (v) {
                state.useXtc = v;
                onStateChanged();
              },
              children: [
                _buildSliderRow(
                  context,
                  label: 'Probability',
                  value: state.xtcProbability,
                  min: 0.0,
                  max: 1.0,
                  divisions: 20,
                  onChanged: (v) {
                    state.xtcProbability = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Threshold',
                  value: state.xtcThreshold,
                  min: 0.0,
                  max: 0.5,
                  divisions: 50,
                  onChanged: (v) {
                    state.xtcThreshold = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Min Keep',
                  value: state.xtcMinKeep.toDouble(),
                  min: 1,
                  max: 10,
                  divisions: 9,
                  displayValue: state.xtcMinKeep.toString(),
                  onChanged: (v) {
                    state.xtcMinKeep = v.round();
                    onStateChanged();
                  },
                ),
              ],
            ),

            // Penalties
            _SamplerStepSection(
              title: 'Repetition Penalties',
              tooltip: 'Discourage repeated tokens',
              enabled: state.usePenalties,
              onEnabledChanged: (v) {
                state.usePenalties = v;
                onStateChanged();
              },
              children: [
                _buildSliderRow(
                  context,
                  label: 'Last N',
                  value: state.penaltyLastN.toDouble(),
                  min: 0,
                  max: 256,
                  divisions: 32,
                  displayValue: state.penaltyLastN.toString(),
                  onChanged: (v) {
                    state.penaltyLastN = v.round();
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Repeat',
                  value: state.penaltyRepeat,
                  min: 1.0,
                  max: 2.0,
                  divisions: 20,
                  onChanged: (v) {
                    state.penaltyRepeat = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Frequency',
                  value: state.penaltyFreq,
                  min: 0.0,
                  max: 2.0,
                  divisions: 40,
                  onChanged: (v) {
                    state.penaltyFreq = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Presence',
                  value: state.penaltyPresent,
                  min: 0.0,
                  max: 2.0,
                  divisions: 40,
                  onChanged: (v) {
                    state.penaltyPresent = v;
                    onStateChanged();
                  },
                ),
              ],
            ),

            // DRY
            _SamplerStepSection(
              title: 'DRY (Don\'t Repeat Yourself)',
              tooltip: 'Reduces repetitive sequences',
              enabled: state.useDry,
              onEnabledChanged: (v) {
                state.useDry = v;
                onStateChanged();
              },
              children: [
                _buildSliderRow(
                  context,
                  label: 'Multiplier',
                  value: state.dryMultiplier,
                  min: 0.0,
                  max: 2.0,
                  divisions: 40,
                  onChanged: (v) {
                    state.dryMultiplier = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Base',
                  value: state.dryBase,
                  min: 1.0,
                  max: 3.0,
                  divisions: 40,
                  onChanged: (v) {
                    state.dryBase = v;
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Allowed Len',
                  value: state.dryAllowedLength.toDouble(),
                  min: 1,
                  max: 10,
                  divisions: 9,
                  displayValue: state.dryAllowedLength.toString(),
                  onChanged: (v) {
                    state.dryAllowedLength = v.round();
                    onStateChanged();
                  },
                ),
                _buildSliderRow(
                  context,
                  label: 'Last N',
                  value: state.dryPenaltyLastN.toDouble(),
                  min: 64,
                  max: 512,
                  divisions: 28,
                  displayValue: state.dryPenaltyLastN.toString(),
                  onChanged: (v) {
                    state.dryPenaltyLastN = v.round();
                    onStateChanged();
                  },
                ),
              ],
            ),

            // Grammar
            _SamplerStepSection(
              title: 'Grammar Constraint',
              tooltip: 'Enforce structured output with GBNF grammar',
              enabled: state.useGrammar,
              onEnabledChanged: (v) {
                state.useGrammar = v;
                onStateChanged();
              },
              children: [
                TextField(
                  decoration: const InputDecoration(
                    labelText: 'Grammar (GBNF)',
                    border: OutlineInputBorder(),
                    hintText: 'root ::= "yes" | "no"',
                    isDense: true,
                  ),
                  maxLines: 3,
                  style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
                  onChanged: (v) {
                    state.grammar = v;
                    onStateChanged();
                  },
                  controller: TextEditingController(text: state.grammar),
                ),
                const SizedBox(height: 8),
                TextField(
                  decoration: const InputDecoration(
                    labelText: 'Root Rule',
                    border: OutlineInputBorder(),
                    isDense: true,
                  ),
                  onChanged: (v) {
                    state.grammarRoot = v;
                    onStateChanged();
                  },
                  controller: TextEditingController(text: state.grammarRoot),
                ),
              ],
            ),

            const Divider(),

            // Finalizer selection
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    'Output Mode (Finalizer):',
                    style: TextStyle(fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: SamplerFinalizer.values.map((f) => ChoiceChip(
                          label: Text(f.label),
                          selected: state.finalizer == f,
                          onSelected: (selected) {
                            if (selected) {
                              state.finalizer = f;
                              onStateChanged();
                            }
                          },
                        )).toList(),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    state.finalizer.description,
                    style: TextStyle(
                      fontSize: 12,
                      color: Colors.grey.shade600,
                      fontStyle: FontStyle.italic,
                    ),
                  ),
                ],
              ),
            ),

            // Mirostat parameters (shown only for mirostat finalizers)
            if (state.finalizer == SamplerFinalizer.mirostatV1 ||
                state.finalizer == SamplerFinalizer.mirostatV2) ...[
              const Divider(),
              Text(
                'Mirostat Parameters',
                style: TextStyle(
                  fontWeight: FontWeight.bold,
                  color: Theme.of(context).colorScheme.primary,
                ),
              ),
              const SizedBox(height: 8),
              _buildSliderRow(
                context,
                label: 'Tau (target)',
                value: state.mirostatTau,
                min: 1.0,
                max: 10.0,
                divisions: 18,
                onChanged: (v) {
                  state.mirostatTau = v;
                  onStateChanged();
                },
              ),
              _buildSliderRow(
                context,
                label: 'Eta (rate)',
                value: state.mirostatEta,
                min: 0.01,
                max: 0.5,
                divisions: 49,
                onChanged: (v) {
                  state.mirostatEta = v;
                  onStateChanged();
                },
              ),
              if (state.finalizer == SamplerFinalizer.mirostatV1)
                _buildSliderRow(
                  context,
                  label: 'M (candidates)',
                  value: state.mirostatM.toDouble(),
                  min: 10,
                  max: 200,
                  divisions: 19,
                  displayValue: state.mirostatM.toString(),
                  onChanged: (v) {
                    state.mirostatM = v.round();
                    onStateChanged();
                  },
                ),
            ],

            const Divider(),

            // Current config description
            Container(
              padding: const EdgeInsets.all(8),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(4),
              ),
              child: Row(
                children: [
                  const Icon(Icons.info_outline, size: 16),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      state.describe(),
                      style: const TextStyle(
                        fontSize: 12,
                        fontFamily: 'monospace',
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildSliderRow(
    BuildContext context, {
    required String label,
    required double value,
    required double min,
    required double max,
    required int divisions,
    required ValueChanged<double> onChanged,
    String? displayValue,
  }) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          SizedBox(
            width: 80,
            child: Text(label, style: const TextStyle(fontSize: 12)),
          ),
          Expanded(
            child: SliderTheme(
              data: SliderThemeData(
                trackHeight: 2,
                thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
                overlayShape: const RoundSliderOverlayShape(overlayRadius: 12),
              ),
              child: Slider(
                value: value,
                min: min,
                max: max,
                divisions: divisions,
                onChanged: onChanged,
              ),
            ),
          ),
          SizedBox(
            width: 50,
            child: Text(
              displayValue ?? value.toStringAsFixed(2),
              style: const TextStyle(fontFamily: 'monospace', fontSize: 11),
            ),
          ),
        ],
      ),
    );
  }
}

/// Collapsible section for a sampler step with enable/disable toggle.
class _SamplerStepSection extends StatelessWidget {
  final String title;
  final String tooltip;
  final bool enabled;
  final ValueChanged<bool> onEnabledChanged;
  final List<Widget> children;

  const _SamplerStepSection({
    required this.title,
    required this.tooltip,
    required this.enabled,
    required this.onEnabledChanged,
    required this.children,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Tooltip(
                message: tooltip,
                child: Text(
                  title,
                  style: TextStyle(
                    fontWeight: FontWeight.w500,
                    color: enabled ? null : Colors.grey,
                  ),
                ),
              ),
            ),
            Switch(
              value: enabled,
              onChanged: onEnabledChanged,
              materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
            ),
          ],
        ),
        if (enabled)
          Padding(
            padding: const EdgeInsets.only(left: 8, bottom: 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: children,
            ),
          ),
        const SizedBox(height: 4),
      ],
    );
  }
}
