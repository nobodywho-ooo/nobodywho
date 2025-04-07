using UnityEngine;
using UnityEditor;

namespace NobodyWho {
    [CustomEditor(typeof(Model))]
    public class ModelEditor : Editor {
        [MenuItem("GameObject/NobodyWho/Model", false, 10)]
        static void CreateModel(MenuCommand menuCommand) {
            GameObject go = new GameObject("NobodyWho Model");
            go.AddComponent<Model>();
            GameObjectUtility.SetParentAndAlign(go, menuCommand.context as GameObject);
            Undo.RegisterCreatedObjectUndo(go, "Create NobodyWho Model");
            Selection.activeObject = go;
        }

        public override void OnInspectorGUI() {
            Model model = (Model)target;

            EditorGUILayout.BeginHorizontal();
            EditorGUILayout.PropertyField(serializedObject.FindProperty("modelPath"));
            
            if (GUILayout.Button("Browse", GUILayout.Width(60))) {
                string startPath = Application.dataPath;
                string path = EditorUtility.OpenFilePanel("Select GGUF Model", startPath, "gguf");
                if (!string.IsNullOrEmpty(path)) {
                    serializedObject.FindProperty("modelPath").stringValue = path;
                    serializedObject.ApplyModifiedProperties();
                }
            }
            EditorGUILayout.EndHorizontal();

            EditorGUILayout.PropertyField(serializedObject.FindProperty("useGpuIfAvailable"));
            
            serializedObject.ApplyModifiedProperties();
        }
    }
} 