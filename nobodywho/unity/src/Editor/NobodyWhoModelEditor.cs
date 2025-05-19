using UnityEditor;
using UnityEngine;

namespace NobodyWho
{
    [CustomEditor(typeof(Model))]
    public class ModelEditor : UnityEditor.Editor
    {
        [MenuItem("GameObject/NobodyWho/Model", false, 10)]
        static void CreateModel(MenuCommand menuCommand)
        {
            GameObject gameObject = new GameObject("NobodyWho Model");
            gameObject.AddComponent<Model>();
            GameObjectUtility.SetParentAndAlign(gameObject, menuCommand.context as GameObject);
            Undo.RegisterCreatedObjectUndo(gameObject, "Create NobodyWho Model");
            Selection.activeObject = gameObject;
        }

        public override void OnInspectorGUI()
        {
            Model model = (Model)target;

            EditorGUILayout.BeginHorizontal();
            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("modelPath"),
                new GUIContent("Model Path", "Path to the GGUF model file")
            );

            if (GUILayout.Button("Browse", GUILayout.Width(60)))
            {
                string startPath = Application.dataPath;
                string path = EditorUtility.OpenFilePanel("Select GGUF Model", startPath, "gguf");
                if (!string.IsNullOrEmpty(path))
                {
                    serializedObject.FindProperty("modelPath").stringValue = path;
                    serializedObject.ApplyModifiedProperties();
                }
            }
            EditorGUILayout.EndHorizontal();

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("useGpuIfAvailable"),
                new GUIContent(
                    "Use GPU If Available",
                    "Whether to use GPU acceleration if available"
                )
            );

            serializedObject.ApplyModifiedProperties();
        }
    }

    [CustomEditor(typeof(Chat))]
    public class ChatEditor : UnityEditor.Editor
    {
        [MenuItem("GameObject/NobodyWho/Chat", false, 12)]
        static void CreateChat(MenuCommand menuCommand)
        {
            GameObject gameObject = new GameObject("NobodyWho Chat");
            var chat = gameObject.AddComponent<Chat>();
            GameObjectUtility.SetParentAndAlign(gameObject, menuCommand.context as GameObject);
            Undo.RegisterCreatedObjectUndo(gameObject, "Create NobodyWho Chat");
            Selection.activeObject = gameObject;
        }

        public override void OnInspectorGUI()
        {
            serializedObject.Update();
            EditorGUI.BeginChangeCheck();
            var modelProp = serializedObject.FindProperty("model");
            EditorGUILayout.PropertyField(
                modelProp,
                new GUIContent("Model", "The model node for the chat")
            );
            if (
                modelProp.objectReferenceValue == null
                || !(modelProp.objectReferenceValue is Model)
            )
            {
                EditorGUILayout.HelpBox("A Model component is required.", MessageType.Error);
            }

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("systemPrompt"),
                new GUIContent(
                    "System Prompt",
                    "The system prompt for the chat, this is the basic instructions for the LLM's behavior"
                )
            );

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("stopWords"),
                new GUIContent(
                    "Stop Words",
                    "Stop words to stop generation at these specified tokens, seperated by commas"
                )
            );

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("contextLength"),
                new GUIContent(
                    "Context Length",
                    "Maximum number of tokens that can be stored in the chat history. Higher values use more VRAM, but allow for longer 'short term memory' for the LLM"
                )
            );

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("use_grammar"),
                new GUIContent("Use Grammar", "Enable grammar-based output formatting")
            );

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("grammar"),
                new GUIContent("Grammar", "Grammar rules to structure the model's output")
            );

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("onToken"),
                new GUIContent("On Token", "Triggered when a new token is received from the LLM")
            );

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("onComplete"),
                new GUIContent(
                    "On Complete",
                    "Triggered when the LLM has finished generating the response"
                )
            );

            serializedObject.ApplyModifiedProperties();
        }
    }

    [CustomEditor(typeof(Embedding))]
    public class EmbeddingEditor : UnityEditor.Editor
    {
        [MenuItem("GameObject/NobodyWho/Embedding", false, 11)]
        static void CreateEmbedding(MenuCommand menuCommand)
        {
            GameObject gameObject = new GameObject("NobodyWho Embedding");
            var embedding = gameObject.AddComponent<Embedding>();
            GameObjectUtility.SetParentAndAlign(gameObject, menuCommand.context as GameObject);
            Undo.RegisterCreatedObjectUndo(gameObject, "Create NobodyWho Embedding");
            Selection.activeObject = gameObject;
        }

        public override void OnInspectorGUI()
        {
            serializedObject.Update();
            EditorGUI.BeginChangeCheck();

            var modelProp = serializedObject.FindProperty("model");
            EditorGUILayout.PropertyField(
                modelProp,
                new GUIContent(
                    "Model",
                    "The model node for the embedding. Required for comparing text similarity without exact matching."
                )
            );
            if (
                modelProp.objectReferenceValue == null
                || !(modelProp.objectReferenceValue is Model)
            )
            {
                EditorGUILayout.HelpBox("A Model component is required.", MessageType.Error);
            }

            EditorGUILayout.Space(10);
            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("onEmbeddingComplete"),
                new GUIContent(
                    "On Embedding Complete",
                    "Triggered when the embedding has finished generating. Returns the embedding as a float array."
                )
            );

            EditorGUILayout.PropertyField(
                serializedObject.FindProperty("onError"),
                new GUIContent(
                    "On Error",
                    "Triggered when an error occurs during embedding generation."
                )
            );

            serializedObject.ApplyModifiedProperties();
        }
    }
}
