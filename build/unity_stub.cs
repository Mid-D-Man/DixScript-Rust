// Minimal stand-ins for the Unity types the popup code uses. Shapes follow UnityCsReference 2022.3.13f1.
#pragma warning disable CS0067, CS0649, CS0169
using System;
using System.Collections.Generic;

namespace UnityEngine
{
    public enum KeyCode { None = 0, Return = 13, Escape = 27, KeypadEnter = 271 }
    [Flags] public enum EventModifiers { None = 0, Shift = 1, Control = 2, Alt = 4, Command = 8 }
    public struct Color { public float r, g, b, a; public Color(float r, float g, float b, float a) { this.r = r; this.g = g; this.b = b; this.a = a; } public Color(float r, float g, float b) { this.r = r; this.g = g; this.b = b; a = 1f; } }
    public struct Color32
    {
        public byte r, g, b, a;
        public Color32(byte r, byte g, byte b, byte a) { this.r = r; this.g = g; this.b = b; this.a = a; }
        public static implicit operator Color(Color32 c) => new Color(c.r / 255f, c.g / 255f, c.b / 255f, c.a / 255f);
    }
    public enum HideFlags { None = 0, HideAndDontSave = 61 }
    public enum ScaleMode { StretchToFill, ScaleAndCrop, ScaleToFit }
    public enum TextureFormat { RGBA32 = 4 }
    public enum AudioType { UNKNOWN = 0, MPEG = 13, OGGVORBIS = 14, WAV = 20 }
    public class Object { public HideFlags hideFlags { get; set; } public static void DestroyImmediate(Object obj) { } }
    public class Texture : Object { public int width { get; } public int height { get; } }
    public class Texture2D : Texture { public Texture2D(int width, int height, TextureFormat textureFormat, bool mipChain) { } }
    public static class ImageConversion { public static bool LoadImage(this Texture2D tex, byte[] data) => true; }
    public sealed class AudioClip : Object { public float length { get; } }
    public static class Debug { public static void LogWarning(object o) { } public static void LogException(System.Exception e) { } }
}

namespace UnityEditor
{
    [System.AttributeUsage(System.AttributeTargets.Method)] public class MenuItemAttribute : System.Attribute { public MenuItemAttribute(string itemName) { } }
    public static class AssetDatabase
    {
        public static string GetAssetPath(UnityEngine.Object assetObject) => "";
        public static string GenerateUniqueAssetPath(string path) => path;
        public static UnityEngine.Object LoadMainAssetAtPath(string assetPath) => null;
        public static void Refresh() { }
    }
    public static class Selection { public static UnityEngine.Object activeObject { get; set; } }
    public static class EditorGUIUtility { public static void PingObject(UnityEngine.Object obj) { } }
    public class EditorWindow { }
    public static class EditorApplication
    {
        public delegate void CallbackFunction();
        public static CallbackFunction delayCall;
        public static CallbackFunction update;
    }
    public static class EditorUtility
    {
        public static string OpenFilePanel(string title, string directory, string extension) => "";
        public static string SaveFilePanel(string title, string directory, string defaultName, string extension) => "";
        public static void OpenWithDefaultApp(string fileName) { }
        public static void FocusProjectWindow() { }
    }
}

namespace UnityEngine.Networking
{
    public class UnityWebRequestAsyncOperation { }
    public class DownloadHandler : System.IDisposable { public void Dispose() { } }
    public class UnityWebRequest : System.IDisposable
    {
        public enum Result { InProgress = 0, Success = 1, ConnectionError = 2, ProtocolError = 3, DataProcessingError = 4 }
        public Result result { get; }
        public string error { get; }
        public bool isDone { get; }
        public UnityWebRequestAsyncOperation SendWebRequest() => null;
        public void Abort() { }
        public void Dispose() { }
    }
    public static class UnityWebRequestMultimedia { public static UnityWebRequest GetAudioClip(string uri, AudioType audioType) => null; }
    public class DownloadHandlerAudioClip : DownloadHandler { public static AudioClip GetContent(UnityWebRequest www) => null; }
}

namespace UnityEngine.UIElements
{
    public enum TrickleDown { NoTrickleDown = 0, TrickleDown = 1 }
    public enum Position { Relative, Absolute }
    public enum Justify { FlexStart, Center, FlexEnd, SpaceBetween, SpaceAround }
    public enum Align { Auto, FlexStart, Center, FlexEnd, Stretch }
    public enum DisplayStyle { Flex, None }
    public enum ScrollerVisibility { Auto, AlwaysVisible, Hidden }
    public enum ScrollViewMode { Vertical, Horizontal, VerticalAndHorizontal }

    public struct StyleLength { public static implicit operator StyleLength(float v) => default; }
    public struct StyleFloat  { public static implicit operator StyleFloat(float v) => default; }
    public struct StyleEnum<T> where T : struct, Enum { public static implicit operator StyleEnum<T>(T v) => default; }

    public interface IStyle
    {
        StyleEnum<Position> position { get; set; }
        StyleLength left { get; set; }
        StyleLength top { get; set; }
        StyleLength right { get; set; }
        StyleLength bottom { get; set; }
        StyleLength width { get; set; }
        StyleLength height { get; set; }
        StyleFloat flexShrink { get; set; }
        StyleEnum<Justify> justifyContent { get; set; }
        StyleEnum<Align> alignItems { get; set; }
        StyleEnum<DisplayStyle> display { get; set; }
    }

    public interface IVisualElementScheduledItem { IVisualElementScheduledItem ExecuteLater(long delayMs); }
    public interface IVisualElementScheduler { IVisualElementScheduledItem Execute(Action updateEvent); }

    public interface IEventHandler { }
    public abstract class EventBase { public IEventHandler target { get; } public void StopImmediatePropagation() { } public void StopPropagation() { } }
    public abstract class EventBase<T> : EventBase where T : EventBase<T>, new() { }
    public class KeyDownEvent : EventBase<KeyDownEvent> { public KeyCode keyCode { get; } public EventModifiers modifiers { get; } }
    public class ClickEvent : EventBase<ClickEvent> { }
    public class ChangeEvent<T> : EventBase<ChangeEvent<T>> { public T newValue { get; } }
    public delegate void EventCallback<TEventType>(TEventType evt);

    public interface INotifyValueChanged<T> { T value { get; set; } void SetValueWithoutNotify(T newValue); }
    public static class NotifyExt
    {
        public static bool RegisterValueChangedCallback<T>(this INotifyValueChanged<T> control, EventCallback<ChangeEvent<T>> callback) => true;
    }

    public class VisualElement : IEventHandler
    {
        public IStyle style { get; }
        public IVisualElementScheduler schedule { get; }
        public bool focusable { get; set; }
        public string tooltip { get; set; }
        public bool enabledSelf { get; }
        public VisualElement parent { get; }
        public void Add(VisualElement child) { }
        public void Clear() { }
        public void RemoveFromHierarchy() { }
        public void AddToClassList(string c) { }
        public void RemoveFromClassList(string c) { }
        public void EnableInClassList(string c, bool enable) { }
        public void SetEnabled(bool value) { }
        public void Focus() { }
        public void RegisterCallback<T>(EventCallback<T> callback, TrickleDown useTrickleDown = TrickleDown.NoTrickleDown) where T : EventBase<T>, new() { }
    }

    public class Image : VisualElement { public Texture image { get; set; } public ScaleMode scaleMode { get; set; } }
    public class ScrollView : VisualElement { public ScrollView() { } public ScrollView(ScrollViewMode mode) { } }
    public class TextElement : VisualElement { public virtual string text { get; set; } }
    public class Label : TextElement { public Label() { } public Label(string text) { } }
    public class Button : TextElement { public Button() { } public Button(Action clickEvent) { } }
    public class TextField : VisualElement, INotifyValueChanged<string>
    {
        public string value { get; set; }
        public bool multiline { get; set; }
        public int maxLength { get; set; }
        public void SetValueWithoutNotify(string newValue) { }
        public bool SetVerticalScrollerVisibility(ScrollerVisibility sv) => true;
    }
}

namespace UnityEditor.UIElements
{
    public class ColorField : UnityEngine.UIElements.VisualElement, UnityEngine.UIElements.INotifyValueChanged<UnityEngine.Color>
    {
        public bool showEyeDropper { get; set; }
        public bool showAlpha { get; set; }
        public bool hdr { get; set; }
        public UnityEngine.Color value { get; set; }
        public void SetValueWithoutNotify(UnityEngine.Color newValue) { }
    }
}
