using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// The player bar, as the GTK app's (ui.rs layout): the song (cover, title, names) at the start;
/// Shuffle, Previous, Play, Next, Repeat with the times and the seek bar in the middle; mute and the
/// volume at the end. The heart and the quality badge come with liking (stage 2). Under 860 px the
/// volume slider makes room; under 600 the bar keeps only the song and Previous/Play/Next.
/// </summary>
public sealed class PlayerBar : Grid
{
    private const double Narrow = 860, Phone = 600;
    private readonly EngineHub hub;
    private readonly SeekModel seek = new();
    private readonly Border art = new() { Width = 56, Height = 56, CornerRadius = new CornerRadius(8) };
    private readonly TextBlock title = new() { Text = "Nothing playing", FontWeight = Microsoft.UI.Text.FontWeights.Bold, TextTrimming = TextTrimming.CharacterEllipsis, MaxLines = 1 };
    private readonly TextBlock byline = new() { Opacity = 0.62, FontSize = 13, TextTrimming = TextTrimming.CharacterEllipsis, MaxLines = 1 };
    private readonly Button shuffle, previous, play, next, repeat, mute;
    private readonly FontIcon playIcon = new() { Glyph = "\uE768", FontSize = 18 };
    private readonly FontIcon repeatIcon = new() { Glyph = "\uE8EE", FontSize = 15 };
    private readonly FontIcon muteIcon = new() { Glyph = "\uE767", FontSize = 15 };
    private readonly Slider bar = new() { Minimum = 0, Maximum = 1, StepFrequency = 0.1, IsThumbToolTipEnabled = false, VerticalAlignment = VerticalAlignment.Center };
    private readonly Slider volume = new() { Minimum = 0, Maximum = 100, Width = 120, IsThumbToolTipEnabled = false, VerticalAlignment = VerticalAlignment.Center };
    private readonly TextBlock at = new() { FontSize = 12, Opacity = 0.7, VerticalAlignment = VerticalAlignment.Center, MinWidth = 36, TextAlignment = TextAlignment.Right };
    private readonly TextBlock length = new() { FontSize = 12, Opacity = 0.7, VerticalAlignment = VerticalAlignment.Center, MinWidth = 36 };
    private readonly StackPanel songWords;
    private bool setting;
    private double unmuted = 1;
    private string artFor = "";

    public PlayerBar(EngineHub hub)
    {
        this.hub = hub;
        Padding = new Thickness(14, 8, 14, 10);
        ColumnSpacing = 16;
        ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star), MinWidth = 160 });
        ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(2, GridUnitType.Star), MaxWidth = 640 });
        ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });

        // the song
        art.Background = (Brush)Application.Current.Resources["CardBrush"];
        songWords = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        songWords.Children.Add(title);
        songWords.Children.Add(byline);
        var start = new Grid { ColumnSpacing = 12 };
        start.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        start.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        start.Children.Add(art);
        SetColumn(songWords, 1);
        start.Children.Add(songWords);
        Children.Add(start);

        // the controls and the seek bar
        shuffle = Round(new FontIcon { Glyph = "\uE8B1", FontSize = 15 }, "Shuffle", () => hub.Engine.SetShuffle(!hub.Engine.Now().Shuffle));
        previous = Round(new FontIcon { Glyph = "\uE892", FontSize = 15 }, "Previous", () => hub.Engine.Skip(-1));
        play = Round(playIcon, "Play", () => hub.Engine.TogglePlay(), accent: true);
        next = Round(new FontIcon { Glyph = "\uE893", FontSize = 15 }, "Next", () => hub.Engine.Skip(1));
        repeat = Round(repeatIcon, "Repeat: off", () => hub.Engine.CycleRepeat());
        var controls = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6, HorizontalAlignment = HorizontalAlignment.Center };
        foreach (var b in new[] { shuffle, previous, play, next, repeat }) controls.Children.Add(b);
        var timeline = new Grid { ColumnSpacing = 8 };
        timeline.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        timeline.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        timeline.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        timeline.Children.Add(at);
        SetColumn(bar, 1);
        timeline.Children.Add(bar);
        SetColumn(length, 2);
        timeline.Children.Add(length);
        var middle = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        middle.Children.Add(controls);
        middle.Children.Add(timeline);
        SetColumn(middle, 1);
        Children.Add(middle);
        bar.AddHandler(PointerPressedEvent, new PointerEventHandler((_, _) => seek.BeginDrag()), true);
        bar.AddHandler(PointerReleasedEvent, new PointerEventHandler((_, _) => hub.Engine.Seek(seek.EndDrag(bar.Value))), true);
        bar.AddHandler(PointerCaptureLostEvent, new PointerEventHandler((_, _) => hub.Engine.Seek(seek.EndDrag(bar.Value))), true);
        bar.ValueChanged += (_, e) =>
        {
            if (setting) return;
            seek.Drag(e.NewValue);
            at.Text = SeekModel.Clock(e.NewValue);
        };

        // the volume
        mute = Round(muteIcon, "Mute", ToggleMute);
        var end = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6, HorizontalAlignment = HorizontalAlignment.Right, VerticalAlignment = VerticalAlignment.Center };
        end.Children.Add(mute);
        end.Children.Add(volume);
        SetColumn(end, 2);
        Children.Add(end);
        volume.ValueChanged += (_, e) =>
        {
            if (setting) return;
            hub.Engine.SetVolume(e.NewValue / 100);
        };

        SizeChanged += (_, e) => Fit(e.NewSize.Width);
        hub.Change += kind =>
        {
            if (kind == ChangeKind.Now) Render();
            else if (kind == ChangeKind.Position) Position();
        };
        Render();
    }

    private static Button Round(FontIcon icon, string tip, Action click, bool accent = false)
    {
        var b = new Button
        {
            Content = icon,
            Width = accent ? 44 : 36,
            Height = accent ? 44 : 36,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(999),
            BorderThickness = new Thickness(0),
        };
        if (accent) b.Style = (Style)Application.Current.Resources["AccentButtonStyle"];
        else b.Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        ToolTipService.SetToolTip(b, tip);
        b.Click += (_, _) => click();
        return b;
    }

    private void Fit(double width)
    {
        volume.Visibility = width < Narrow ? Visibility.Collapsed : Visibility.Visible;
        var phone = width < Phone ? Visibility.Collapsed : Visibility.Visible;
        foreach (var e in new UIElement[] { shuffle, repeat, mute, byline, at, length }) e.Visibility = phone;
    }

    private void ToggleMute()
    {
        var level = hub.Engine.Now().Volume;
        if (level > 0)
        {
            unmuted = level;
            hub.Engine.SetVolume(0);
        }
        else hub.Engine.SetVolume(unmuted > 0 ? unmuted : 1);
    }

    private void Render()
    {
        var now = hub.Engine.Now();
        var song = now.Song;
        title.Text = song?.Title ?? "Nothing playing";
        byline.Text = song?.Byline ?? "";
        foreach (var c in new Control[] { play, previous, bar }) c.IsEnabled = song != null;
        previous.IsEnabled = now.CanPrevious;
        next.IsEnabled = now.CanNext;
        playIcon.Glyph = now.Playing ? "\uE769" : "\uE768";
        ToolTipService.SetToolTip(play, now.Playing ? "Pause" : "Play");
        ToolTipService.SetToolTip(shuffle, now.Shuffle ? "Shuffle: on" : "Shuffle");
        shuffle.Foreground = now.Shuffle ? (Brush)Application.Current.Resources["AccentTextBrush"] : null;
        var (glyph, tip) = now.Repeat switch
        {
            RepeatMode.All => ("\uE8EE", "Repeat: the whole queue"),
            RepeatMode.One => ("\uE8ED", "Repeat: this song"),
            _ => ("\uE8EE", "Repeat: off"),
        };
        repeatIcon.Glyph = glyph;
        ToolTipService.SetToolTip(repeat, tip);
        repeat.Opacity = now.Repeat == RepeatMode.Off ? 0.55 : 1;
        repeat.Foreground = now.Repeat == RepeatMode.Off ? null : (Brush)Application.Current.Resources["AccentTextBrush"];
        muteIcon.Glyph = now.Volume <= 0 ? "\uE74F" : now.Volume < 0.34 ? "\uE993" : now.Volume < 0.67 ? "\uE994" : "\uE767";
        ToolTipService.SetToolTip(mute, now.Volume <= 0 ? "Unmute" : "Mute");
        setting = true;
        volume.Value = now.Volume * 100;
        bar.Maximum = Math.Max(1, now.Length);
        setting = false;
        length.Text = SeekModel.Clock(now.Length);
        Position();
        var source = song?.Art ?? "";
        if (source != artFor)
        {
            artFor = source;
            _ = LoadArt(source);
        }
    }

    private void Position()
    {
        var now = hub.Engine.Now();
        seek.Position(now.At);
        setting = true;
        bar.Maximum = Math.Max(1, now.Length);
        bar.Value = Math.Min(seek.Shown, bar.Maximum);
        setting = false;
        at.Text = SeekModel.Clock(seek.Shown);
    }

    private async Task LoadArt(string source)
    {
        var image = await ArtCache.LoadAsync(hub.Engine, source, 112);
        if (source != artFor) return;
        art.Background = image != null ? new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill } : (Brush)Application.Current.Resources["CardBrush"];
    }
}
