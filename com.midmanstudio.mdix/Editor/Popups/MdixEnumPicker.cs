using System;
using System.Collections.Generic;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The content of the enum popup: the members of the enum as declared in the file's @ENUMS section, with the
    /// current one marked. Click one to choose it; the popup's Apply writes it. A member shows its number when the
    /// declaration gave one (<c>RARE = 2</c>).
    ///
    /// The runtime does not list an enum's members, so the list comes from the source text. An enum declared in an
    /// imported file is not found there, and its values stay read-only.
    /// </summary>
    internal sealed class MdixEnumPicker
    {
        private readonly MdixEnumDef                 _def;
        private readonly string                      _initial;
        private readonly VisualElement               _root;
        private readonly List<KeyValuePair<Button, string>> _items = new List<KeyValuePair<Button, string>>();

        private string _selected;

        public VisualElement Root { get { return _root; } }

        /// <summary>The chosen member's name, without the enum's name.</summary>
        public string Selected { get { return _selected; } }

        /// <summary>True once a member other than the current one is chosen.</summary>
        public bool IsChanged { get { return !string.Equals(_selected, _initial, StringComparison.Ordinal); } }

        /// <summary>Raised when the choice changes, so the popup can enable Apply.</summary>
        public event Action Changed;

        public MdixEnumPicker(MdixEnumDef def, string currentMember)
        {
            _def      = def;
            _initial  = currentMember ?? string.Empty;
            _selected = _initial;

            _root = new VisualElement();
            _root.AddToClassList("mdix-enum");

            var caption = new Label("Choose a member of " + def.Name);
            caption.AddToClassList("mdix-pop__hint");
            _root.Add(caption);

            var list = new ScrollView(ScrollViewMode.Vertical);
            list.AddToClassList("mdix-enum__list");

            foreach (var member in def.Members)
            {
                var name   = member.Name;
                var button = new Button(() => Choose(name))
                {
                    text = member.Value == null ? name : name + "   = " + member.Value,
                };
                button.AddToClassList("mdix-btn");
                button.AddToClassList("mdix-enum__item");

                _items.Add(new KeyValuePair<Button, string>(button, name));
                list.Add(button);
            }

            _root.Add(list);
            Refresh();
        }

        private void Choose(string member)
        {
            if (string.Equals(_selected, member, StringComparison.Ordinal)) return;

            _selected = member;
            Refresh();

            var handler = Changed;
            if (handler != null) handler();
        }

        private void Refresh()
        {
            foreach (var item in _items)
                item.Key.EnableInClassList("mdix-enum__item--on", string.Equals(item.Value, _selected, StringComparison.Ordinal));
        }
    }
}
