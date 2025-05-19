using UnityEditor;
using System.IO;
using System;

namespace NobodyWho.Editor
{
    public class PackageBuilder
    {
        public static void CreatePackageForCI(string projectPath, string outputPath)
        {
            try
            {
                string packagePath = Path.Combine(projectPath, "Packages", "com.nobodywho.core");
                
                // Validate paths
                if (!Directory.Exists(packagePath))
                {
                    throw new DirectoryNotFoundException($"Package directory not found at: {packagePath}");
                }

                // Ensure output directory exists
                Directory.CreateDirectory(outputPath);

                // Export the package
                string packageOutputPath = Path.Combine(outputPath, "com.nobodywho.core.unitypackage");
                AssetDatabase.ExportPackage(packagePath, packageOutputPath, 
                    ExportPackageOptions.Recurse | ExportPackageOptions.IncludeDependencies);

                Debug.Log($"Package created successfully at: {packageOutputPath}");
            }
            catch (Exception ex)
            {
                Debug.LogError($"Failed to create package in CI: {ex.Message}");
                throw;
            }
        }
    }
}