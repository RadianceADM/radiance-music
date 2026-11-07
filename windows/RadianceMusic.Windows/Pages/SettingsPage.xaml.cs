using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Pages;

/// <summary>
/// The beta's Settings (stage 4a, the owner's dialog "A Settings page"): what the Linux app keeps in its main
/// menu, in Windows' own place (the sidebar's gear), one section at a time (the owner's rule for long pages):
/// Playing (Autoplay), Storage (where the songs are kept, the space used, moving them) and Help (How to Use
/// It, What's New, Look for Updates, Report a Problem…, Rate Radiance Music, About). Soft corners; every
/// button with its icon.
/// </summary>
public sealed partial class SettingsPage : Page
{
    private static readonly string[] Sections = { "Playing", "Storage", "Help" };
    private EngineHub? hub;
    private readonly HelpWords w = Ffi.HelpWords();
    private readonly SelectorBar bar = new();
    private readonly StackPanel body = new() { Spacing = 16 };
    private StackPanel section = new() { Spacing = 12 };
    /// <summary>Where the section shows: a new one crossfades over the old (rule 16).</summary>
    private readonly Grid sections = new();
    private int shown;

    /// <summary>The section open now (the screenshots' --act section:N).</summary>
    internal int Section
    {
        get => shown;
        set
        {
            if (value < 0 || value >= Sections.Length) return;
            bar.SelectedItem = bar.Items[value];
        }
    }

    public SettingsPage()
    {
        InitializeComponent();
        NavigationCacheMode = NavigationCacheMode.Required;
        body.Children.Add(PageKit.Title("Settings"));
        foreach (var s in Sections) bar.Items.Add(new SelectorBarItem { Text = s });
        bar.SelectionChanged += (_, _) =>
        {
            shown = Math.Max(0, bar.Items.IndexOf(bar.SelectedItem));
            section = new StackPanel { Spacing = 12 };
            Render();
            Motion.Crossfade(sections, section);
        };
        body.Children.Add(bar);
        sections.Children.Add(section);
        body.Children.Add(sections);
        Content = PageKit.Column(body);
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub == null && e.Parameter is EngineHub h)
        {
            hub = h;
            hub.Change += kind =>
            {
                if (kind == ChangeKind.Storage && shown == 1) Render();
            };
        }
        if (bar.SelectedItem == null) bar.SelectedItem = bar.Items[0];
        else Render();
    }

    private void Render()
    {
        if (hub == null) return;
        section.Children.Clear();
        switch (shown)
        {
            case 0: Playing(); break;
            case 1: Storage(); break;
            default: Help(); break;
        }
    }

    private static Border Card(UIElement content) => new()
    {
        Child = content,
        CornerRadius = new CornerRadius(12),
        Padding = new Thickness(20, 16, 20, 16),
        Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
    };

    // ---- Playing

    private void Playing()
    {
        var autoplay = new ToggleSwitch { Header = w.Autoplay, IsOn = hub!.Engine.Autoplay() };
        autoplay.Toggled += (_, _) => hub.Engine.SetAutoplay(autoplay.IsOn);
        section.Children.Add(Card(autoplay));
        section.Children.Add(Card(BetweenSongs()));
    }

    /// <summary>
    /// Between Songs (stage 4a-2, the Linux app's main-menu section): No Transition, Crossfade or DJ Mix (by the Beat),
    /// and the crossfade length (1 to 12 s, saved once the slider rests, as the Mixer's effects). The slider dims with
    /// No Transition (it's there, only not used).
    /// </summary>
    private StackPanel BetweenSongs()
    {
        var panel = new StackPanel { Spacing = 10 };
        var modes = new RadioButtons { Header = w.BetweenSongs };
        foreach (var label in new[] { w.NoTransition, w.Crossfade, w.DjMix }) modes.Items.Add(label);
        modes.SelectedIndex = hub!.Engine.Transition() switch { TransitionMode.Crossfade => 1, TransitionMode.DjMix => 2, _ => 0 };
        var label = new TextBlock { Text = Ffi.FadeLabel(hub.Engine.Fade()), Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] };
        var fade = new Slider { Minimum = 1, Maximum = 12, StepFrequency = 1, Value = hub.Engine.Fade(), IsThumbToolTipEnabled = false, Width = 320, HorizontalAlignment = HorizontalAlignment.Left };
        fade.IsEnabled = modes.SelectedIndex != 0;
        modes.SelectionChanged += (_, _) =>
        {
            var mode = modes.SelectedIndex switch { 1 => TransitionMode.Crossfade, 2 => TransitionMode.DjMix, _ => TransitionMode.Off };
            if (mode == hub.Engine.Transition()) return;
            hub.Engine.SetTransition(mode);
            fade.IsEnabled = mode != TransitionMode.Off;
        };
        var later = DispatcherQueue.CreateTimer();
        later.Interval = TimeSpan.FromMilliseconds(120);
        later.IsRepeating = false;
        later.Tick += (_, _) => hub.Engine.SetFade(fade.Value);
        fade.ValueChanged += (_, e) =>
        {
            label.Text = Ffi.FadeLabel(e.NewValue);
            later.Stop();
            later.Start();
        };
        panel.Children.Add(modes);
        panel.Children.Add(label);
        panel.Children.Add(fade);
        return panel;
    }

    // ---- Storage (Task 11)

    private void Storage()
    {
        var st = hub!.Engine.Storage();
        if (st.Counting && !st.Moving && st.Rows.Length == 0 && !counted)
        {
            counted = true;
            hub.Engine.StorageCount();
        }
        var kept = new StackPanel { Spacing = 8 };
        kept.Children.Add(PageKit.Subtitle(w.KeptGroup));
        kept.Children.Add(new TextBlock { Text = w.Folder, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
        kept.Children.Add(new TextBlock { Text = st.Folder, IsTextSelectionEnabled = true, TextWrapping = TextWrapping.Wrap, Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] });
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var change = PageKit.IconButton("", w.Change, () => _ = ChangeFolder(st.Folder));
        change.IsEnabled = !st.Moving;
        buttons.Children.Add(change);
        buttons.Children.Add(PageKit.IconButton("", w.OpenFolder, () => OpenFolder(st.Folder)));
        kept.Children.Add(buttons);
        section.Children.Add(Card(kept));

        var space = new StackPanel { Spacing = 8 };
        space.Children.Add(PageKit.Subtitle(w.SpaceGroup));
        space.Children.Add(PageKit.Note(st.Total));
        foreach (var r in st.Rows)
        {
            var row = new Grid { ColumnSpacing = 12 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var words = new StackPanel();
            words.Children.Add(new TextBlock { Text = r.Title });
            if (r.Subtitle.Length > 0) words.Children.Add(new TextBlock { Text = r.Subtitle, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] });
            var size = new TextBlock { Text = r.Size, VerticalAlignment = VerticalAlignment.Center };
            Grid.SetColumn(size, 1);
            row.Children.Add(words);
            row.Children.Add(size);
            space.Children.Add(row);
        }
        section.Children.Add(Card(space));
    }

    private bool counted;

    private static void OpenFolder(string folder)
    {
        try
        {
            Directory.CreateDirectory(folder);
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo("explorer.exe", $"\"{folder}\"") { UseShellExecute = false });
        }
        catch (Exception e)
        {
            Crash.Note("open the music folder: " + e.Message);
        }
    }

    /// <summary>Change…: a folder picked (Windows' own picker), then "Move your songs?" as the Linux app asks.</summary>
    private async Task ChangeFolder(string from)
    {
        var picker = new Windows.Storage.Pickers.FolderPicker { SuggestedStartLocation = Windows.Storage.Pickers.PickerLocationId.MusicLibrary };
        picker.FileTypeFilter.Add("*");
        if (App.Window != null) WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(App.Window));
        var chosen = await picker.PickSingleFolderAsync();
        if (chosen == null) return;
        await ConfirmMove(from, chosen.Path);
    }

    /// <summary>"Move your songs?" for `chosen` (the screenshots' --act storage-move:PATH comes here too).</summary>
    internal async Task ConfirmMove(string from, string chosen)
    {
        var to = Ffi.StorageTarget(chosen);
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = w.MoveTitle,
            Content = new TextBlock { Text = Ffi.MoveText(from, to), TextWrapping = TextWrapping.Wrap },
            PrimaryButtonText = w.MoveWord,
            CloseButtonText = w.Cancel,
            DefaultButton = ContentDialogButton.Primary,
            CornerRadius = new CornerRadius(16),
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) hub!.Engine.MoveMusic(to);
    }

    // ---- Help

    private void Help()
    {
        var help = new StackPanel { Spacing = 8 };
        help.Children.Add(PageKit.IconButton("", w.GuideTitle, () => _ = Dialogs.GuideDialog.Show(hub!, XamlRoot)));
        help.Children.Add(PageKit.IconButton("", w.WhatsNew, () => _ = Dialogs.WhatsNewDialog.Show(XamlRoot)));
        help.Children.Add(PageKit.IconButton("", w.Look, () => hub!.Engine.LookForUpdates(true)));
        help.Children.Add(PageKit.IconButton("", w.ReportTitle + "…", () => _ = Dialogs.ReportDialog.Show(hub!, XamlRoot)));
        help.Children.Add(PageKit.IconButton("", w.RateTitle, () => _ = Dialogs.RatingDialog.Show(hub!, XamlRoot, false)));
        section.Children.Add(Card(help));

        var about = new StackPanel { Spacing = 6 };
        about.Children.Add(PageKit.Subtitle($"{w.AppName} (beta) {Ffi.BetaVersion()}"));
        about.Children.Add(PageKit.Note(w.AboutText));
        about.Children.Add(PageKit.Note($"{w.Developer} · {w.Copyright} · GPL-3.0"));
        var links = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        links.Children.Add(new HyperlinkButton { Content = "GitHub", NavigateUri = new Uri(w.Website) });
        links.Children.Add(new HyperlinkButton { Content = w.ReportIssue, NavigateUri = new Uri(w.Issues) });
        about.Children.Add(links);
        about.Children.Add(new TextBlock { Text = w.CreditsMusic, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], Margin = new Thickness(0, 8, 0, 0) });
        about.Children.Add(Credits(w.Sources));
        about.Children.Add(new TextBlock { Text = w.CreditsBuilt, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
        about.Children.Add(Credits(w.BuiltWith));
        section.Children.Add(Card(about));
    }

    /// <summary>"Name https://…" pairs as links.</summary>
    private static StackPanel Credits(string[] pairs)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        foreach (var p in pairs)
        {
            var at = p.LastIndexOf(' ');
            if (at <= 0) continue;
            row.Children.Add(new HyperlinkButton { Content = p[..at], NavigateUri = new Uri(p[(at + 1)..]) });
        }
        return row;
    }
}
