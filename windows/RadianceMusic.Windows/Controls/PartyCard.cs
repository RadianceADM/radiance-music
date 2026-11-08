using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using RadianceMusic.Native;
using RadianceMusic.Pages;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// A server page's listening party, as the Linux app's render_party_card and render_in_party (stage 4c-2). Not in its
/// party: what's on (asking / nobody's / NAME is hosting, what plays, how many listen) with Start a Party (and Let
/// everyone play, pause and skip) or Join. In it: whose party, the Playing / Paused / Quiet pill, the sync line, what
/// plays, Play / Pause and Skip (the host's, or everyone's when allowed), Hear It Here, the six reactions and the last
/// minute's, who's listening, Up next (Earlier / Later / Take it out), the host's control switch, Leave and End (it asks).
/// Made again only when what it shows changes; the sync line alone changes every second.
/// </summary>
internal sealed class PartyCard : Grid
{
    // (a Grid holding the card: WinUI's Border is sealed)
    private readonly Border frame = new();
    private readonly EngineHub hub;
    private readonly string address;
    private readonly StackPanel body = new() { Spacing = 10 };
    private readonly TextBlock sync = new() { TextWrapping = TextWrapping.Wrap, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] };
    private string shown = "";

    public PartyCard(EngineHub hub, string address)
    {
        this.hub = hub;
        this.address = address;
        frame.CornerRadius = new CornerRadius(16);
        frame.Padding = new Thickness(20, 16, 20, 18);
        frame.BorderThickness = new Thickness(1);
        frame.BorderBrush = (Brush)Application.Current.Resources["CardStrokeColorDefaultBrush"];
        frame.Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"];
        frame.Child = body;
        Children.Add(frame);
        Render();
    }

    /// <summary>The card as it is now (from Change.Party, Change.Servers).</summary>
    public void Render()
    {
        var card = hub.Engine.PartyCard(address);
        if (card.InIt)
        {
            var p = hub.Engine.Party();
            sync.Text = p.SyncLine;
            sync.Foreground = (Brush)Application.Current.Resources[p.Lost ? "SystemFillColorCautionBrush" : "TextFillColorSecondaryBrush"];
            var key = "in|" + string.Join("|", p.Heading, p.Pill, p.Now?.Key, p.Control, p.Host, p.CanEnd, p.EveryoneControls, p.Hearing, p.ControlsNote,
                string.Join(",", p.Reactions.Select(r => r.Tip + r.Emoji)), string.Join(",", p.Members),
                string.Join(",", p.UpNext.Select(n => $"{n.Entry}{n.Earlier}{n.Later}{n.TakeOut}")));
            if (key == shown) return;
            shown = key;
            Accent(true);
            body.Children.Clear();
            InParty(p);
            return;
        }
        var cardKey = "card|" + string.Join("|", card.Heading, card.Line, card.CanStart, card.Join, card.Joining, card.Leaves);
        if (cardKey == shown) return;
        shown = cardKey;
        Accent(card.Join.Length > 0);
        body.Children.Clear();
        Card(card);
    }

    /// <summary>A party on: the card's accent edge (soft, rounded; no sharp visuals).</summary>
    private void Accent(bool on)
    {
        frame.BorderBrush = (Brush)Application.Current.Resources[on ? "AccentFillColorDefaultBrush" : "CardStrokeColorDefaultBrush"];
    }

    private static StackPanel Heading(string text)
    {
        var line = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        line.Children.Add(new FontIcon { Glyph = "", FontSize = 20 });
        line.Children.Add(new TextBlock { Text = text, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"], TextWrapping = TextWrapping.Wrap });
        return line;
    }

    private static TextBlock Dim(string text) => new()
    {
        Text = text,
        TextWrapping = TextWrapping.Wrap,
        Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
    };

    /// <summary>Not in this server's party: start one, or join the one that's on.</summary>
    private void Card(PartyCardView c)
    {
        var w = Ffi.PartyWords();
        body.Children.Add(Heading(c.Heading));
        body.Children.Add(Dim(c.Line));
        if (c.CanStart)
        {
            var everyone = new CheckBox { Content = w.LetEveryone };
            var start = PageKit.IconButton("", w.StartParty, () => { }, accent: true);
            start.Click += (_, _) =>
            {
                start.IsEnabled = false;
                hub.Engine.PartyStart(address, everyone.IsChecked == true);
            };
            body.Children.Add(everyone);
            body.Children.Add(start);
        }
        if (c.Join.Length > 0)
        {
            var join = PageKit.IconButton("", c.Join, () => hub.Engine.PartyJoin(address), accent: true);
            join.IsEnabled = !c.Joining;
            body.Children.Add(join);
            if (c.Leaves.Length > 0) body.Children.Add(Dim(c.Leaves));
        }
    }

    /// <summary>The party you're in.</summary>
    private void InParty(PartyView p)
    {
        var w = Ffi.PartyWords();
        var top = new Grid { ColumnSpacing = 10 };
        top.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        top.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        top.Children.Add(Heading(p.Heading));
        var pill = new Border
        {
            CornerRadius = new CornerRadius(999),
            Padding = new Thickness(12, 3, 12, 4),
            VerticalAlignment = VerticalAlignment.Center,
            Background = (Brush)Application.Current.Resources[p.Playing ? "AccentFillColorDefaultBrush" : "ControlFillColorSecondaryBrush"],
            Child = new TextBlock
            {
                Text = p.Pill,
                Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"],
                Foreground = (Brush)Application.Current.Resources[p.Playing ? "TextOnAccentFillColorPrimaryBrush" : "TextFillColorPrimaryBrush"],
            },
        };
        Grid.SetColumn(pill, 1);
        top.Children.Add(pill);
        body.Children.Add(top);
        body.Children.Add(sync);

        // what plays
        var now = new Grid { ColumnSpacing = 14 };
        now.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        now.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var art = new Border { Width = 64, Height = 64, CornerRadius = new CornerRadius(10), Background = (Brush)Application.Current.Resources["ControlFillColorSecondaryBrush"] };
        art.Child = new FontIcon { Glyph = "", FontSize = 26 };
        now.Children.Add(art);
        var words = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        if (p.Now is { } song)
        {
            words.Children.Add(new TextBlock { Text = song.Title, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], TextWrapping = TextWrapping.Wrap });
            words.Children.Add(Dim(song.Subtitle));
            if (song.Art.Length > 0) _ = LoadArt(art, song.Art);
        }
        else
        {
            words.Children.Add(new TextBlock { Text = w.NothingPlaying, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
            words.Children.Add(Dim(w.AddASong));
        }
        Grid.SetColumn(words, 1);
        now.Children.Add(words);
        body.Children.Add(now);

        // its controls (the host's, or everyone's when the host allows), and hearing it here
        var controls = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        if (p.Control)
        {
            var pp = PageKit.IconButton(p.Playing ? "" : "", p.Playing ? w.Pause : w.Play, () => hub.Engine.PartyPlay(!p.Playing));
            pp.IsEnabled = p.Now != null || p.UpNext.Length > 0;
            var skip = PageKit.IconButton("", w.Skip, () => hub.Engine.PartySkip());
            skip.IsEnabled = p.Now != null;
            controls.Children.Add(pp);
            controls.Children.Add(skip);
        }
        var hear = PageKit.IconButton(p.Hearing ? "" : "", p.Hearing ? w.StopHearing : w.HearItHere, () => hub.Engine.PartyHear(!p.Hearing));
        ToolTipService.SetToolTip(hear, w.HearTip);
        controls.Children.Add(hear);
        body.Children.Add(controls);
        if (p.ControlsNote.Length > 0) body.Children.Add(Caption(p.ControlsNote));

        // reactions: the server's own pictures (round, soft)
        var reacts = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        foreach (var r in w.Reactions)
        {
            var b = new Button { Padding = new Thickness(6), CornerRadius = new CornerRadius(999), Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent), BorderThickness = new Thickness(0), Content = Icon(r.Icon, 26) };
            ToolTipService.SetToolTip(b, r.Tip);
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(b, r.Tip);
            var emoji = r.Emoji;
            b.Click += (_, _) => hub.Engine.PartyReact(emoji);
            Motion.AddPressScale(b);
            reacts.Children.Add(b);
        }
        body.Children.Add(reacts);
        if (p.Reactions.Length > 0)
        {
            var fresh = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
            foreach (var r in p.Reactions) fresh.Children.Add(Chip(Icon(r.Icon, 20), r.By, r.Tip));
            body.Children.Add(fresh);
        }

        // who's listening
        body.Children.Add(Caption(p.Listening, strong: true));
        var people = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        foreach (var m in p.Members)
            people.Children.Add(Chip(new PersonPicture { DisplayName = m.Split(" · ")[0], Width = 24, Height = 24 }, m, null));
        body.Children.Add(people);

        // up next
        body.Children.Add(Caption(w.UpNext, strong: true));
        if (p.UpNext.Length == 0)
        {
            body.Children.Add(new TextBlock { Text = w.NothingUpNext, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
            body.Children.Add(Dim(w.NothingUpNextSub));
        }
        for (var i = 0; i < p.UpNext.Length; i++)
        {
            var n = p.UpNext[i];
            var to = (uint)i;
            var row = new Grid { ColumnSpacing = 4 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var text = new StackPanel { Spacing = 1, VerticalAlignment = VerticalAlignment.Center };
            text.Children.Add(new TextBlock { Text = n.Song.Title, TextTrimming = TextTrimming.CharacterEllipsis });
            text.Children.Add(new TextBlock { Text = n.Song.Subtitle, TextTrimming = TextTrimming.CharacterEllipsis, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] });
            row.Children.Add(text);
            var tools = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 2 };
            if (p.Control)
            {
                tools.Children.Add(Tool("", w.Earlier, n.Earlier, () => hub.Engine.PartyMove(n.Entry, to == 0 ? 0 : to - 1)));
                tools.Children.Add(Tool("", w.Later, n.Later, () => hub.Engine.PartyMove(n.Entry, to + 1)));
            }
            if (n.TakeOut) tools.Children.Add(Tool("", w.TakeOut, true, () => hub.Engine.PartyRemove(n.Entry)));
            Grid.SetColumn(tools, 1);
            row.Children.Add(tools);
            body.Children.Add(row);
        }

        // the host's: who controls it, the end; everyone's: leave
        var end = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, Margin = new Thickness(0, 4, 0, 0) };
        if (p.Host)
            end.Children.Add(PageKit.IconButton("", p.EveryoneControls ? w.OnlyIControl : w.LetEveryoneControl, () => hub.Engine.PartyAllow(!p.EveryoneControls)));
        end.Children.Add(PageKit.IconButton("", w.LeaveParty, () => hub.Engine.PartyLeave()));
        if (p.CanEnd)
        {
            var stop = PageKit.IconButton("", w.EndParty, () => { });
            // End asks first (a flyout: a dialog may be open already)
            var confirm = new StackPanel { Spacing = 10, MaxWidth = 340 };
            confirm.Children.Add(new TextBlock { Text = w.EndHeading, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
            confirm.Children.Add(new TextBlock { Text = w.EndBody, TextWrapping = TextWrapping.Wrap });
            var sure = PageKit.IconButton("", w.EndParty, () => { });
            sure.Background = (Brush)Application.Current.Resources["SystemFillColorCriticalBrush"];
            sure.Foreground = new SolidColorBrush(Microsoft.UI.Colors.White);
            var cancel = PageKit.IconButton("", w.Cancel, () => { });
            var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, HorizontalAlignment = HorizontalAlignment.Right };
            buttons.Children.Add(cancel);
            buttons.Children.Add(sure);
            confirm.Children.Add(buttons);
            var flyout = new Flyout { Content = confirm };
            FlyoutBase.SetAttachedFlyout(stop, flyout);
            stop.Click += (_, _) => FlyoutBase.ShowAttachedFlyout(stop);
            cancel.Click += (_, _) => flyout.Hide();
            sure.Click += (_, _) =>
            {
                flyout.Hide();
                hub.Engine.PartyEnd();
            };
            end.Children.Add(stop);
        }
        body.Children.Add(end);
    }

    private static TextBlock Caption(string text, bool strong = false) => new()
    {
        Text = text,
        TextWrapping = TextWrapping.Wrap,
        Style = (Style)Application.Current.Resources[strong ? "BodyStrongTextBlockStyle" : "CaptionTextBlockStyle"],
        Foreground = (Brush)Application.Current.Resources[strong ? "TextFillColorPrimaryBrush" : "TextFillColorSecondaryBrush"],
    };

    private static Button Tool(string glyph, string tip, bool on, Action click)
    {
        var b = new Button
        {
            Content = new FontIcon { Glyph = glyph, FontSize = 14 },
            Padding = new Thickness(8),
            CornerRadius = new CornerRadius(999),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            IsEnabled = on,
        };
        ToolTipService.SetToolTip(b, tip);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(b, tip);
        b.Click += (_, _) => click();
        return b;
    }

    /// <summary>A round chip: a picture and a name (a reaction, or a listener).</summary>
    private static Border Chip(FrameworkElement picture, string text, string? tip)
    {
        var line = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        line.Children.Add(picture);
        line.Children.Add(new TextBlock { Text = text, VerticalAlignment = VerticalAlignment.Center });
        var chip = new Border
        {
            CornerRadius = new CornerRadius(999),
            Padding = new Thickness(6, 3, 12, 3),
            Background = (Brush)Application.Current.Resources["ControlFillColorSecondaryBrush"],
            Child = line,
        };
        if (tip != null) ToolTipService.SetToolTip(chip, tip);
        return chip;
    }

    /// <summary>A reaction's picture (icons\rms-react-*.svg beside the app; one the app doesn't know: a soft smile).</summary>
    private static FrameworkElement Icon(string name, double size)
    {
        var file = Path.Combine(AppContext.BaseDirectory, "icons", name + ".svg");
        if (!File.Exists(file)) return new FontIcon { Glyph = "", FontSize = size * 0.8, Width = size, Height = size };
        return new Image { Source = new SvgImageSource(new Uri(file)) { RasterizePixelWidth = size, RasterizePixelHeight = size }, Width = size, Height = size };
    }

    private async Task LoadArt(Border art, string source)
    {
        var image = await ArtCache.LoadAsync(hub.Engine, source, 128);
        if (image != null) art.Child = new Image { Source = image, Stretch = Stretch.UniformToFill };
    }
}
