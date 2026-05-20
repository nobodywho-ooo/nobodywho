import siteConfig from '@generated/docusaurus.config';
import type * as PrismNamespace from 'prismjs';
import type {Optional} from 'utility-types';

function registerGDScript(Prism: typeof PrismNamespace): void {
  Prism.languages.gdscript = {
    comment: /#.*/,
    'triple-quoted-string': {
      pattern: /"""[\s\S]*?"""/,
      greedy: true,
      alias: 'string',
    },
    string: [
      {pattern: /"(?:[^"\\]|\\.)*"/, greedy: true},
      {pattern: /'(?:[^'\\]|\\.)*'/, greedy: true},
    ],
    annotation: {
      pattern: /@\w+/,
      alias: 'decorator',
    },
    'node-path': {
      pattern: /\$(?:"[^"]*"|[A-Za-z_]\w*(?:\/[A-Za-z_]\w*)*)/,
      alias: 'variable',
    },
    keyword:
      /\b(?:and|as|assert|await|break|breakpoint|class|class_name|const|continue|elif|else|enum|export|extends|for|func|if|in|is|match|not|null|onready|or|pass|preload|return|self|setget|signal|static|super|tool|var|while|yield)\b/,
    boolean: /\b(?:true|false)\b/,
    'class-name': {
      pattern:
        /\b(?:String|int|float|bool|void|Array|Dictionary|Vector2|Vector2i|Vector3|Vector3i|Vector4|Color|Rect2|Transform2D|Transform3D|Basis|AABB|Plane|Quaternion|NodePath|RID|Callable|Signal|PackedStringArray|PackedInt32Array|PackedInt64Array|PackedFloat32Array|PackedFloat64Array|PackedVector2Array|PackedVector3Array|PackedByteArray|PackedColorArray|PackedScene|Resource|Node|Node2D|Node3D|Control|Object|RefCounted|StringName)\b/,
      alias: 'class-name',
    },
    builtin:
      /\b(?:print|prints|printt|print_rich|push_error|push_warning|str|abs|ceil|floor|round|clamp|sign|min|max|typeof|len|range|load|preload|weakref|is_instance_valid|inst_to_dict|dict_to_inst|randi|randf|randfn|remap|lerp|lerpf|inverse_lerp|snapped|wrap|ease|smoothstep|move_toward|PI|TAU|INF|NAN)\b/,
    function: {
      pattern: /\b[a-z_]\w*(?=\s*\()/,
      alias: 'function',
    },
    number: [
      /\b0x[\da-fA-F](?:_?[\da-fA-F])*\b/,
      /\b0b[01](?:_?[01])*\b/,
      /\b\d[\d_]*(?:\.[\d_]+)?(?:[eE][+-]?\d[\d_]*)?\b/,
    ],
    operator:
      /->|:=|\.\.|&&|\|\||<<|>>|[+\-*/%&|^~!=<>]=?/,
    punctuation: /[{}[\]();,.:]/,
  };
}

export default function prismIncludeLanguages(
  PrismObject: typeof PrismNamespace,
): void {
  const {
    themeConfig: {prism},
  } = siteConfig;
  const {additionalLanguages} = prism as {additionalLanguages: string[]};

  const PrismBefore = globalThis.Prism;
  globalThis.Prism = PrismObject;

  additionalLanguages.forEach((lang) => {
    if (lang === 'php') {
      require('prismjs/components/prism-markup-templating.js');
    }
    require(`prismjs/components/prism-${lang}`);
  });

  // Register custom languages
  registerGDScript(PrismObject);

  delete (globalThis as Optional<typeof globalThis, 'Prism'>).Prism;
  if (typeof PrismBefore !== 'undefined') {
    globalThis.Prism = PrismObject;
  }
}
