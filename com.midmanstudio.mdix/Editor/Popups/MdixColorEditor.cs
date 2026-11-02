using System;
using UnityEditor.UIElements;
using UnityEngine;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The content of the colour popup: Unity's own colour field (click the swatch for the picker with its wheel and
    /// sliders and the eyedropper), the colour's channels, and a text box with the hex value. Each of the three
    /// follows the others.
    ///
    /// A colour picked with the wheel can be any of 16 million values, while <c>#RGB</c> and <c>#RGBA</c> hold 4096
    /// (they repeat each digit). So the colour is written in the short form only when it was written that way and the
    /// picked colour fits it exactly; otherwise it is written in full. Alpha is written when the colour had it or is
    /// not fully opaque. A typed value that is not a colour is marked and blocks Apply.
    /// </summary>
    internal sealed class MdixColorEditor
    {
        private readonly string        _initial;
        private readonly bool          _hadAlpha;
        private readonly bool          _preferShort;
        private readonly byte          _r0, _g0, _b0, _a0;
        private readonly VisualElement _root;
        private readonly ColorField    _field;
        private readonly TextField     _text;
        private readonly Label         _channels;

        private byte _r, _g, _b, _a;
        private bool _syncing;

        public VisualElement Root        { get { return _root; } }
        public VisualElement FocusTarget { get { return _text; } }

        /// <summary>The colour as it would be written.</summary>
        public string Text { get; private set; }

        /// <summary>What is wrong with the typed text, or null.</summary>
        public string Error { get; private set; }

        /// <summary>True when the text is a colour and differs from the one the file has.</summary>
        public bool CanApply { get { return Error == null && (_r != _r0 || _g != _g0 || _b != _b0 || _a != _a0); } }

        /// <summary>Raised after each change, so the popup can update its error line and Apply.</summary>
        public event Action Changed;

        public MdixColorEditor(string initialLiteral)
        {
            _initial = initialLiteral ?? string.Empty;

            if (!MdixColorText.TryParse(_initial, out _r0, out _g0, out _b0, out _a0, out _hadAlpha, out _preferShort))
            {
                _r0 = _g0 = _b0 = 0; _a0 = 255; _hadAlpha = false; _preferShort = false;
            }
            _r = _r0; _g = _g0; _b = _b0; _a = _a0;
            Text = _initial;

            _root = new VisualElement();
            _root.AddToClassList("mdix-color");

            var row = new VisualElement();
            row.AddToClassList("mdix-color__row");

            _field = new ColorField { showAlpha = true, showEyeDropper = true, hdr = false };
            _field.AddToClassList("mdix-color__field");
            _field.tooltip = "Click to open Unity's colour picker.";
            _field.SetValueWithoutNotify(new Color32(_r, _g, _b, _a));
            _field.RegisterValueChangedCallback(e => OnPicked(e.newValue));
            row.Add(_field);

            _text = new TextField { value = _initial, maxLength = 9, tooltip = "#RGB, #RGBA, #RRGGBB or #RRGGBBAA" };
            _text.AddToClassList("mdix-color__text");
            _text.RegisterValueChangedCallback(e => OnTyped(e.newValue));
            row.Add(_text);

            _root.Add(row);

            _channels = new Label();
            _channels.AddToClassList("mdix-pop__hint");
            _root.Add(_channels);

            var hint = new Label("Click the colour for Unity's picker: a wheel or box, sliders and an eyedropper. Or type the hex value.");
            hint.AddToClassList("mdix-pop__hint");
            _root.Add(hint);

            ShowChannels();
        }

        // ── Inputs ────────────────────────────────────────────────────────────

        // A colour chosen in Unity's picker.
        private void OnPicked(Color picked)
        {
            if (_syncing) return;

            _r = ToByte(picked.r); _g = ToByte(picked.g); _b = ToByte(picked.b); _a = ToByte(picked.a);
            Error = null;
            Text  = MdixColorText.Format(_r, _g, _b, _a, _hadAlpha, _preferShort);

            _syncing = true;
            try
            {
                _text.SetValueWithoutNotify(Text);
                _text.RemoveFromClassList("mdix-pop__input--bad");
            }
            finally { _syncing = false; }

            ShowChannels();
            RaiseChanged();
        }

        // Hex typed into the text box.
        private void OnTyped(string typed)
        {
            if (_syncing) return;

            byte r, g, b, a; bool hasAlpha, isShort;
            if (!MdixColorText.TryParse(typed, out r, out g, out b, out a, out hasAlpha, out isShort))
            {
                Error = "Enter a colour like #F80, #F80C, #FF8800 or #FF8800CC.";
                _text.AddToClassList("mdix-pop__input--bad");
                RaiseChanged();
                return;
            }

            _r = r; _g = g; _b = b; _a = a;
            Error = null;
            Text  = typed.Trim();
            _text.RemoveFromClassList("mdix-pop__input--bad");

            _syncing = true;
            try { _field.SetValueWithoutNotify(new Color32(_r, _g, _b, _a)); }
            finally { _syncing = false; }

            ShowChannels();
            RaiseChanged();
        }

        private void ShowChannels()
        {
            _channels.text = "R " + _r + "   G " + _g + "   B " + _b + "   A " + _a;
        }

        private void RaiseChanged()
        {
            var handler = Changed;
            if (handler != null) handler();
        }

        private static byte ToByte(float channel)
        {
            return (byte)Math.Round(Math.Max(0f, Math.Min(1f, channel)) * 255f);
        }
    }
}
