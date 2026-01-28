import 'package:nobodywho/nobodywho.dart' as nobodywho;

/// Sampler preset definitions with labels and descriptions.
enum SamplerPreset {
  defaultSampler('Default', 'Balanced settings for general use'),
  greedy('Greedy', 'Always picks the most likely token (deterministic)'),
  temperatureLow('Temperature (0.3)', 'Low randomness, more focused'),
  temperatureMedium('Temperature (0.7)', 'Balanced randomness'),
  temperatureHigh('Temperature (1.2)', 'High randomness, more creative'),
  topK40('Top-K (40)', 'Sample from top 40 tokens'),
  topP90('Top-P (0.9)', 'Nucleus sampling with p=0.9'),
  dry('DRY', 'Reduces repetition'),
  json('JSON', 'Enforces valid JSON output');

  final String label;
  final String description;
  const SamplerPreset(this.label, this.description);
}

/// Get a SamplerConfig from a preset.
nobodywho.SamplerConfig getSamplerFromPreset(SamplerPreset preset) {
  switch (preset) {
    case SamplerPreset.defaultSampler:
      return nobodywho.SamplerPresets.defaultSampler();
    case SamplerPreset.greedy:
      return nobodywho.SamplerPresets.greedy();
    case SamplerPreset.temperatureLow:
      return nobodywho.SamplerPresets.temperature(temperature: 0.3);
    case SamplerPreset.temperatureMedium:
      return nobodywho.SamplerPresets.temperature(temperature: 0.7);
    case SamplerPreset.temperatureHigh:
      return nobodywho.SamplerPresets.temperature(temperature: 1.2);
    case SamplerPreset.topK40:
      return nobodywho.SamplerPresets.topK(topK: 40);
    case SamplerPreset.topP90:
      return nobodywho.SamplerPresets.topP(topP: 0.9);
    case SamplerPreset.dry:
      return nobodywho.SamplerPresets.dry();
    case SamplerPreset.json:
      return nobodywho.SamplerPresets.json();
  }
}

/// How to finalize the sampler chain.
enum SamplerFinalizer {
  dist('Distribution', 'Sample from probability distribution'),
  greedy('Greedy', 'Always pick most probable token'),
  mirostatV1('Mirostat v1', 'Perplexity-controlled sampling (v1)'),
  mirostatV2('Mirostat v2', 'Perplexity-controlled sampling (v2)');

  final String label;
  final String description;
  const SamplerFinalizer(this.label, this.description);
}

/// State for the custom sampler builder with all available parameters.
class SamplerBuilderState {
  // Temperature step
  bool useTemperature = true;
  double temperature = 0.7;

  // Top-K step
  bool useTopK = true;
  int topK = 40;

  // Top-P (nucleus) step
  bool useTopP = true;
  double topP = 0.9;
  int topPMinKeep = 1;

  // Min-P step
  bool useMinP = false;
  double minP = 0.05;
  int minPMinKeep = 1;

  // Typical-P step
  bool useTypicalP = false;
  double typicalP = 0.9;
  int typicalPMinKeep = 1;

  // XTC (eXclude Top Choices) step
  bool useXtc = false;
  double xtcProbability = 0.5;
  double xtcThreshold = 0.1;
  int xtcMinKeep = 1;

  // Penalties step
  bool usePenalties = false;
  int penaltyLastN = 64;
  double penaltyRepeat = 1.1;
  double penaltyFreq = 0.0;
  double penaltyPresent = 0.0;

  // DRY step
  bool useDry = false;
  double dryMultiplier = 0.8;
  double dryBase = 1.75;
  int dryAllowedLength = 2;
  int dryPenaltyLastN = 256;

  // Grammar step
  bool useGrammar = false;
  String grammar = '';
  String grammarRoot = 'root';

  // Finalizer
  SamplerFinalizer finalizer = SamplerFinalizer.dist;

  // Mirostat parameters (used when finalizer is mirostatV1 or mirostatV2)
  double mirostatTau = 5.0;
  double mirostatEta = 0.1;
  int mirostatM = 100; // Only used for v1

  /// Reset to defaults.
  void reset() {
    useTemperature = true;
    temperature = 0.7;

    useTopK = true;
    topK = 40;

    useTopP = true;
    topP = 0.9;
    topPMinKeep = 1;

    useMinP = false;
    minP = 0.05;
    minPMinKeep = 1;

    useTypicalP = false;
    typicalP = 0.9;
    typicalPMinKeep = 1;

    useXtc = false;
    xtcProbability = 0.5;
    xtcThreshold = 0.1;
    xtcMinKeep = 1;

    usePenalties = false;
    penaltyLastN = 64;
    penaltyRepeat = 1.1;
    penaltyFreq = 0.0;
    penaltyPresent = 0.0;

    useDry = false;
    dryMultiplier = 0.8;
    dryBase = 1.75;
    dryAllowedLength = 2;
    dryPenaltyLastN = 256;

    useGrammar = false;
    grammar = '';
    grammarRoot = 'root';

    finalizer = SamplerFinalizer.dist;
    mirostatTau = 5.0;
    mirostatEta = 0.1;
    mirostatM = 100;
  }

  /// Build the sampler configuration from current state.
  nobodywho.SamplerConfig build() {
    var builder = nobodywho.SamplerBuilder();

    // Add steps in order
    if (useTemperature) {
      builder = builder.temperature(temperature: temperature);
    }

    if (useTopK) {
      builder = builder.topK(topK: topK);
    }

    if (useTopP) {
      builder = builder.topP(topP: topP, minKeep: topPMinKeep);
    }

    if (useMinP) {
      builder = builder.minP(minP: minP, minKeep: minPMinKeep);
    }

    if (useTypicalP) {
      builder = builder.typicalP(typP: typicalP, minKeep: typicalPMinKeep);
    }

    if (useXtc) {
      builder = builder.xtc(
        xtcProbability: xtcProbability,
        xtcThreshold: xtcThreshold,
        minKeep: xtcMinKeep,
      );
    }

    if (usePenalties) {
      builder = builder.penalties(
        penaltyLastN: penaltyLastN,
        penaltyRepeat: penaltyRepeat,
        penaltyFreq: penaltyFreq,
        penaltyPresent: penaltyPresent,
      );
    }

    if (useDry) {
      builder = builder.dry(
        multiplier: dryMultiplier,
        base: dryBase,
        allowedLength: dryAllowedLength,
        penaltyLastN: dryPenaltyLastN,
        seqBreakers: ['\n', ':', '"', '*'],
      );
    }

    if (useGrammar && grammar.isNotEmpty) {
      builder = builder.grammar(
        grammar: grammar,
        triggerOn: null,
        root: grammarRoot,
      );
    }

    // Finalize
    switch (finalizer) {
      case SamplerFinalizer.dist:
        return builder.dist();
      case SamplerFinalizer.greedy:
        return builder.greedy();
      case SamplerFinalizer.mirostatV1:
        return builder.mirostatV1(tau: mirostatTau, eta: mirostatEta, m: mirostatM);
      case SamplerFinalizer.mirostatV2:
        return builder.mirostatV2(tau: mirostatTau, eta: mirostatEta);
    }
  }

  /// Get a description of the current configuration.
  String describe() {
    final parts = <String>[];

    if (useTemperature) parts.add('temp=${temperature.toStringAsFixed(1)}');
    if (useTopK) parts.add('top_k=$topK');
    if (useTopP) parts.add('top_p=${topP.toStringAsFixed(2)}');
    if (useMinP) parts.add('min_p=${minP.toStringAsFixed(2)}');
    if (useTypicalP) parts.add('typ_p=${typicalP.toStringAsFixed(2)}');
    if (useXtc) parts.add('xtc');
    if (usePenalties) parts.add('penalties');
    if (useDry) parts.add('DRY');
    if (useGrammar && grammar.isNotEmpty) parts.add('grammar');

    switch (finalizer) {
      case SamplerFinalizer.greedy:
        parts.add('greedy');
      case SamplerFinalizer.dist:
        parts.add('dist');
      case SamplerFinalizer.mirostatV1:
        parts.add('miro_v1(τ=${mirostatTau.toStringAsFixed(1)})');
      case SamplerFinalizer.mirostatV2:
        parts.add('miro_v2(τ=${mirostatTau.toStringAsFixed(1)})');
    }

    return parts.isEmpty ? 'Empty chain' : parts.join(', ');
  }
}
