import { describe, it, before } from 'node:test';
import assert from 'node:assert/strict';
import {
  Model,
  Chat,
  TokenStream,
  Tool,
  SamplerBuilder,
  SamplerConfig,
  SamplerPresets,
  cosineSimilarity,
  streamTokens,
  createTool,
} from './dist/index.js';

// ---------- Tests that don't need a model ----------

describe('cosineSimilarity', () => {
  it('returns 1.0 for identical vectors', () => {
    const v = [1, 2, 3];
    const sim = cosineSimilarity(v, v);
    assert.ok(Math.abs(sim - 1.0) < 1e-6);
  });

  it('returns 0.0 for orthogonal vectors', () => {
    const sim = cosineSimilarity([1, 0], [0, 1]);
    assert.ok(Math.abs(sim) < 1e-6);
  });

  it('returns -1.0 for opposite vectors', () => {
    const sim = cosineSimilarity([1, 0], [-1, 0]);
    assert.ok(Math.abs(sim - (-1.0)) < 1e-6);
  });
});

describe('Model', () => {
  it('hasGpuBackend returns a boolean', () => {
    const result = Model.hasGpuBackend();
    assert.equal(typeof result, 'boolean');
  });
});

describe('SamplerBuilder', () => {
  it('creates a builder and chains shift steps', () => {
    const config = new SamplerBuilder()
      .topK(40)
      .temperature(0.8)
      .dist();
    assert.ok(config instanceof SamplerConfig);
  });

  it('chains multiple shift steps before sampling', () => {
    const config = new SamplerBuilder()
      .topK(40)
      .topP(0.95, 1)
      .minP(0.05, 1)
      .temperature(0.7)
      .dist();
    assert.ok(config instanceof SamplerConfig);
  });

  it('greedy produces a SamplerConfig', () => {
    const config = new SamplerBuilder().greedy();
    assert.ok(config instanceof SamplerConfig);
  });

  it('mirostatV1 produces a SamplerConfig', () => {
    const config = new SamplerBuilder().mirostatV1(5.0, 0.1, 100);
    assert.ok(config instanceof SamplerConfig);
  });

  it('mirostatV2 produces a SamplerConfig', () => {
    const config = new SamplerBuilder().mirostatV2(5.0, 0.1);
    assert.ok(config instanceof SamplerConfig);
  });

  it('supports DRY sampler', () => {
    const config = new SamplerBuilder()
      .dry(0.0, 1.75, 2, -1, ['\n', ':', '"', '*'])
      .dist();
    assert.ok(config instanceof SamplerConfig);
  });

  it('supports penalties', () => {
    const config = new SamplerBuilder()
      .penalties(-1, 1.1, 0.0, 0.0)
      .dist();
    assert.ok(config instanceof SamplerConfig);
  });
});

describe('SamplerConfig', () => {
  it('toJson returns valid JSON', () => {
    const config = new SamplerBuilder().topK(40).dist();
    const json = config.toJson();
    const parsed = JSON.parse(json);
    assert.ok(Array.isArray(parsed.steps));
  });

  it('fromJson round-trips', () => {
    const original = new SamplerBuilder().topK(40).temperature(0.8).dist();
    const json = original.toJson();
    const restored = SamplerConfig.fromJson(json);
    assert.equal(restored.toJson(), json);
  });

  it('fromJson rejects invalid JSON', () => {
    assert.throws(() => SamplerConfig.fromJson('not json'));
  });
});

describe('SamplerPresets', () => {
  it('defaultSampler returns a config', () => {
    const config = SamplerPresets.defaultSampler();
    assert.ok(config instanceof SamplerConfig);
    assert.equal(typeof config.toJson(), 'string');
  });

  it('greedy returns a config', () => {
    const config = SamplerPresets.greedy();
    assert.ok(config instanceof SamplerConfig);
  });

  it('topK returns a config', () => {
    const config = SamplerPresets.topK(40);
    assert.ok(config instanceof SamplerConfig);
  });

  it('topP returns a config', () => {
    const config = SamplerPresets.topP(0.95);
    assert.ok(config instanceof SamplerConfig);
  });

  it('temperature returns a config', () => {
    const config = SamplerPresets.temperature(0.8);
    assert.ok(config instanceof SamplerConfig);
  });

  it('dry returns a config', () => {
    const config = SamplerPresets.dry();
    assert.ok(config instanceof SamplerConfig);
  });

  it('json returns a config', () => {
    const config = SamplerPresets.json();
    assert.ok(config instanceof SamplerConfig);
  });
});

describe('createTool', () => {
  it('creates a tool with createTool helper', () => {
    const tool = createTool({
      name: 'get_weather',
      description: 'Get weather for a city',
      parameters: [['city', 'string'], ['unit', 'string']],
      call: (city, unit) => JSON.stringify({ temp: 22, city, unit }),
    });
    assert.ok(tool instanceof Tool);
  });

  it('creates a tool with raw Tool constructor', () => {
    const schema = JSON.stringify({
      type: 'object',
      properties: { x: { type: 'integer' } },
      required: ['x'],
    });
    const tool = new Tool('double', 'doubles a number', schema, (argsJson) => {
      const { x } = JSON.parse(argsJson);
      return String(x * 2);
    });
    assert.ok(tool instanceof Tool);
  });
});

// ---------- Tests that need a model ----------

const modelPath = process.env.TEST_MODEL;

describe('Model loading', { skip: !modelPath && 'TEST_MODEL not set' }, () => {
  let model;

  before(async () => {
    model = await Model.load(modelPath, true, null);
  });

  it('loads a model', () => {
    assert.ok(model instanceof Model);
  });

  it('rejects nonexistent model path', async () => {
    await assert.rejects(() => Model.load('/nonexistent/model.gguf', false, null));
  });

  describe('Chat', () => {
    it('creates a chat and streams tokens', async () => {
      const chat = new Chat(model, 'You are helpful. Be brief.', 2048);
      const stream = chat.ask('Say hello in exactly 3 words.');

      assert.ok(stream instanceof TokenStream);

      const tokens = [];
      let token;
      while ((token = await stream.nextToken()) !== null) {
        tokens.push(token);
      }

      assert.ok(tokens.length > 0, 'Should have received at least one token');
      const fullResponse = tokens.join('');
      assert.ok(fullResponse.length > 0, 'Full response should not be empty');
    });

    it('completed returns full response', async () => {
      const chat = new Chat(model, 'You are helpful. Be brief.', 2048);
      const stream = chat.ask('Say hi.');
      const response = await stream.completed();

      assert.ok(typeof response === 'string');
      assert.ok(response.length > 0);
    });

    it('accepts sampler config', () => {
      const sampler = new SamplerBuilder().topK(20).temperature(0.5).dist();
      const chat = new Chat(model, 'You are helpful.', 2048, null, null, sampler);
      assert.ok(chat instanceof Chat);
    });

    it('streamTokens yields tokens via for-await', async () => {
      const chat = new Chat(model, 'You are helpful. Be brief.', 2048);
      const tokens = [];
      for await (const token of streamTokens(chat.ask('Say hi.'))) {
        tokens.push(token);
      }
      assert.ok(tokens.length > 0, 'Should have received at least one token');
      assert.ok(tokens.join('').length > 0, 'Full response should not be empty');
    });

    it('askWithPrompt streams tokens for text parts', async () => {
      const chat = new Chat(model, 'You are helpful. Be brief.', 2048);
      const stream = chat.askWithPrompt([
        { type: 'text', content: 'Say hello in exactly 3 words.' },
      ]);
      assert.ok(stream instanceof TokenStream);
      const response = await stream.completed();
      assert.ok(response.length > 0, 'Response should not be empty');
    });

    it('stopGeneration does not throw', () => {
      const chat = new Chat(model, null, 2048);
      chat.stopGeneration();
    });
  });
});
