using System.IO;
using UnityEditor;
using UnityEngine;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Right-click Create menu items for .mdix files.
    /// Assets → Create → MDIX → [template name]
    /// </summary>
    internal static class CreateMdixMenuItems
    {
        private const string MenuRoot = "Assets/Create/MDIX/";

        // ── Menu items ────────────────────────────────────────────────────────

        [MenuItem(MenuRoot + "Blank File")]
        private static void CreateBlank() =>
            CreateFile("new_config", Templates.Blank);

        [MenuItem(MenuRoot + "Game Enemies")]
        private static void CreateGameEnemies() =>
            CreateFile("enemies", Templates.GameEnemies);

        [MenuItem(MenuRoot + "Inventory Items")]
        private static void CreateInventoryItems() =>
            CreateFile("items", Templates.InventoryItems);

        [MenuItem(MenuRoot + "App Config")]
        private static void CreateAppConfig() =>
            CreateFile("app_config", Templates.AppConfig);

        [MenuItem(MenuRoot + "Multi-Environment Server Config")]
        private static void CreateMultiEnvServer() =>
            CreateFile("server_config", Templates.MultiEnvServer);

        [MenuItem(MenuRoot + "Encrypted Secrets")]
        private static void CreateEncryptedSecrets() =>
            CreateFile("secrets", Templates.EncryptedSecrets);

        [MenuItem(MenuRoot + "Player Save Data")]
        private static void CreatePlayerSave() =>
            CreateFile("player_save", Templates.PlayerSave);

        // ── Context menu on selected asset ────────────────────────────────────

        [MenuItem("Assets/MDIX/Generate ScriptableObject", false, 1200)]
        private static void ContextBake()
        {
            var selected = Selection.activeObject as MdixAsset;
            if (selected != null)
                MdixBakeWizard.Open(selected);
        }

        [MenuItem("Assets/MDIX/Generate ScriptableObject", true)]
        private static bool ContextBakeValidate() =>
            Selection.activeObject is MdixAsset;

        [MenuItem("Assets/MDIX/Open in MDIX Studio", false, 1201)]
        private static void ContextOpenStudio()
        {
            var selected = Selection.activeObject as MdixAsset;
            if (selected != null)
                MdixEditorWindow.OpenWithAsset(selected);
        }

        [MenuItem("Assets/MDIX/Open in MDIX Studio", true)]
        private static bool ContextOpenStudioValidate() =>
            Selection.activeObject is MdixAsset;

        // ── File creation helper ──────────────────────────────────────────────

        private static void CreateFile(string defaultName, string content)
        {
            var path = GetSelectedFolderPath();
            var filePath = AssetDatabase.GenerateUniqueAssetPath(
                $"{path}/{defaultName}.mdix");

            File.WriteAllText(filePath, content);
            AssetDatabase.Refresh();

            var asset = AssetDatabase.LoadAssetAtPath<MdixAsset>(filePath);
            if (asset != null)
            {
                EditorUtility.FocusProjectWindow();
                Selection.activeObject = asset;

                // Trigger rename in the Project window so the user can
                // immediately give the file a meaningful name.
                EditorApplication.delayCall += () =>
                {
                    EditorUtility.FocusProjectWindow();
                    Selection.activeObject = asset;
                    var type = typeof(EditorWindow).Assembly
                        .GetType("UnityEditor.ProjectBrowser");
                    if (type != null)
                    {
                        var window = EditorWindow.GetWindow(type);
                        window?.SendEvent(
                            EditorGUIUtility.CommandEvent("Rename"));
                    }
                };
            }
        }

        private static string GetSelectedFolderPath()
        {
            var path = AssetDatabase.GetAssetPath(Selection.activeObject);

            if (string.IsNullOrEmpty(path))
                return "Assets";

            return Directory.Exists(path)
                ? path
                : Path.GetDirectoryName(path) ?? "Assets";
        }
    }

    // ── Templates ─────────────────────────────────────────────────────────────

    internal static class Templates
    {
        public const string Blank =
@"@CONFIG(
  version -> ""1.0.0""
  author  -> ""YourName""
)

@DATA(

)
";

        public const string GameEnemies =
@"@CONFIG(
  version -> ""1.0.0""
)

@ENUMS(
  AIType { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }
  Rarity { COMMON, UNCOMMON, RARE, LEGENDARY }
)

@QUICKFUNCS(
  ~createEnemy<object>(name, health, damage, ai<enum>, rarity<enum>) {
    return {
      name      = name
      health    = health
      damage    = damage
      armor     = health / 10
      xp        = health / 2
      ai_type   = ai
      rarity    = rarity
      spawn_rate = ai == AIType.BOSS ? 0.01f : 0.3f
    }
  }
)

@DATA(
  spawn_cap   = 12
  respawn_delay = 5000

  enemies::
    createEnemy(""Goblin"",  50,   10,  AIType.AGGRESSIVE, Rarity.COMMON),
    createEnemy(""Orc"",     100,  20,  AIType.AGGRESSIVE, Rarity.UNCOMMON),
    createEnemy(""Troll"",   250,  45,  AIType.AGGRESSIVE, Rarity.RARE),
    createEnemy(""Dragon"",  1000, 150, AIType.BOSS,       Rarity.LEGENDARY)
)
";

        public const string InventoryItems =
@"@CONFIG(
  version -> ""1.0.0""
)

@ENUMS(
  ItemType  { WEAPON, ARMOR, CONSUMABLE, QUEST }
  Rarity    { COMMON, UNCOMMON, RARE, EPIC, LEGENDARY }
)

@QUICKFUNCS(
  ~createItem<object>(id, name, type<enum>, rarity<enum>, value) {
    return {
      id       = id
      name     = name
      type     = type
      rarity   = rarity
      value    = value
      weight   = value / 100
      stackable = type == ItemType.CONSUMABLE ? true : false
    }
  }
)

@DATA(
  max_stack_size = 99

  items::
    createItem(1, ""Iron Sword"",    ItemType.WEAPON,     Rarity.COMMON,    50),
    createItem(2, ""Health Potion"", ItemType.CONSUMABLE, Rarity.COMMON,    25),
    createItem(3, ""Dragon Scale"",  ItemType.ARMOR,      Rarity.LEGENDARY, 5000)
)
";

        public const string AppConfig =
@"@CONFIG(
  version -> ""1.0.0""
)

@ENUMS(
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
  LogLevel    { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 }
)

@DATA(
  app_name<string>       = ""MyGame""
  version<string>        = ""1.0.0""
  environment<enum>      = Environment.DEV
  log_level<enum>        = LogLevel.INFO
  debug_mode<bool>       = false

  server: host = ""localhost"", port = 8080, ssl = false
  features: analytics = true, ads = false, cloud_save = true
)
";

        public const string MultiEnvServer =
@"@CONFIG(
  version -> ""1.0.0""
)

@ENUMS(
  Environment { DEV = 1, STAGING = 2, PROD = 3 }
)

@QUICKFUNCS(
  ~serverConfig<object>(env<enum>, host, pool_size) {
    return {
      host       = host
      port       = 8080
      pool_size  = pool_size
      ssl        = env == Environment.PROD ? true : false
      timeout    = env == Environment.PROD ? 3000 : 10000
      log_level  = env == Environment.DEV  ? ""DEBUG"" : ""WARN""
    }
  }
)

@DATA(
  current_env<enum> = Environment.DEV

  dev:     serverConfig(Environment.DEV,     ""localhost"",        5)
  staging: serverConfig(Environment.STAGING, ""staging.myapi.com"", 20)
  prod:    serverConfig(Environment.PROD,    ""api.myapi.com"",     50)
)
";

        public const string EncryptedSecrets =
@"@CONFIG(
  version -> ""1.0.0""
)

@DLM(
  DCompressor.gzip
  DEncryptor.aes256
)

@DATA(
  api_key          = ""REPLACE_WITH_YOUR_KEY""
  analytics_token  = ""REPLACE_WITH_TOKEN""
  cdn_url          = ""https://cdn.yourgame.com""
)

@SECURITY(
  encryption -> { mode = ""keyfile"", algorithm = ""aes256-gcm"" }
)
";

        public const string PlayerSave =
@"@CONFIG(
  version -> ""1.0.0""
)

@ENUMS(
  GameDifficulty { EASY, NORMAL, HARD, NIGHTMARE }
)

@DATA(
  player_name<string>    = ""Player""
  level<int>             = 1
  experience<int>        = 0
  health<int>            = 100
  max_health<int>        = 100
  currency<int>          = 0
  difficulty<enum>       = GameDifficulty.NORMAL
  play_time_seconds<int> = 0
  last_saved             = 2025-01-01

  position: x = 0.0, y = 0.0, z = 0.0
  flags: tutorial_complete = false, intro_seen = false
)
";
    }
}
