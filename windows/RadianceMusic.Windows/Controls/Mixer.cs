using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;

namespace RadianceMusic.Controls;

/// <summary>
/// The Mixer, as the Linux app's (ui.rs, its ExpanderRow; stage 3), in a pop-up from the bar's Mixer button
/// (owner, 2026-11-06: "Pop-up from the bar"; soft, rounded): the voice buttons (Song,
/// No vocals, Vocals only), a row for each of the song's parts (Drums, Bass, Other, Vocals; Music and Vocals
/// for a song split in two) with its meter, Low cut / High cut / Reverb (one setting, every song), and Reset.
/// Every move is heard at once (the engine's volumes); the meters run only while its pop-up is open.
/// </summary>
internal sealed class Mixer : StackPanel
{
    private static readonly (VoiceMode Mode, string Label)[] Voices = { (VoiceMode.Song, "Song"), (VoiceMode.NoVocals, "No vocals"), (VoiceMode.VocalsOnly, "Vocals only") };
    private readonly EngineHub hub;
    private readonly SelectorBar voices = new();
    private readonly StackPanel partRows = new() { Spacing = 4 };
    /// <summary>"Plays as the full song until it's split." (a voice asked for on a song not split yet).</summary>
    private readonly TextBlock note = new() { TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
    /// <summary>
    /// While the song playing is split, or waits in line for it, whatever asked for it (owner, 2026-11-07/08: "walang
    /// loading indicator sa mixer kung splitting pa", "bakit di ko paren nakikita sa mixer kung splitting pa lang"):
    /// what's happening, a bar (how far a four-part split is; moving for the rest), and Turn Off while it's for the
    /// voice picked (its buttons show Song meanwhile, "diba dapat automatic sya babalek sa song kase di pa downloaded").
    /// </summary>
    private readonly StackPanel splitting = new() { Spacing = 6, Visibility = Visibility.Collapsed, Opacity = 0 };
    private readonly TextBlock splittingWords = new() { TextWrapping = TextWrapping.Wrap, VerticalAlignment = VerticalAlignment.Center };
    private readonly ProgressBar splitBar = new() { IsIndeterminate = true, Minimum = 0, Maximum = 100 };
    private readonly Button turnOff = new() { Padding = new Thickness(10, 3, 10, 4) };
    /// <summary>The song menu's split choices for the song playing ("walang option sa mixer to split 4 ways").</summary>
    private readonly StackPanel splitChoices = new() { Orientation = Orientation.Horizontal, Spacing = 8, Visibility = Visibility.Collapsed };
    private string shownChoices = "-";
    private readonly Dictionary<string, (Slider Slider, ProgressBar Meter)> parts = new();
    private readonly Slider[] fx = new Slider[3];
    private readonly DispatcherQueueTimer meters, fxLater;
    private string shownParts = "";
    private bool setting;

    public Mixer(EngineHub hub)
    {
        this.hub = hub;
        Spacing = 12;
        Width = 360;
        Children.Add(new TextBlock { Text = "Mixer", Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"] });
        foreach (var (_, label) in Voices) voices.Items.Add(new SelectorBarItem { Text = label });
        ToolTipService.SetToolTip(voices, "No vocals: the music without the singer (karaoke). Vocals only: just the voice. Split once per song by Demucs; it carries on to the next songs.");
        voices.SelectionChanged += (_, _) =>
        {
            if (setting) return;
            var i = voices.Items.IndexOf(voices.SelectedItem);
            if (i >= 0) hub.Engine.SetVoice(Voices[i].Mode);
        };
        var body = this;
        body.Children.Add(voices);
        {
            var line = new Grid { ColumnSpacing = 8 };
            line.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            line.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            splittingWords.Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"];
            line.Children.Add(splittingWords);
            var off = turnOff;
            var offWords = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
            offWords.Children.Add(new FontIcon { Glyph = "\uE7E8", FontSize = 12 });
            offWords.Children.Add(new TextBlock { Text = "Turn Off", Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] });
            off.Content = offWords;
            ToolTipService.SetToolTip(off, "Back to the song as it is (the split goes on, for next time)");
            off.Click += (_, _) => hub.Engine.SetVoice(VoiceMode.Song);
            Motion.AddPressScale(off);
            Grid.SetColumn(off, 1);
            line.Children.Add(off);
            splitting.Children.Add(line);
            splitting.Children.Add(splitBar);
            body.Children.Add(splitting);
            body.Children.Add(splitChoices);
        }
        note.Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"];
        note.Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];
        body.Children.Add(note);
        body.Children.Add(partRows);
        var effects = new StackPanel { Spacing = 4 };
        string[] names = { "Low cut", "High cut", "Reverb" };
        for (var i = 0; i < fx.Length; i++)
        {
            var s = new Slider { Minimum = 0, Maximum = 100, StepFrequency = 1, IsThumbToolTipEnabled = false, HorizontalAlignment = HorizontalAlignment.Stretch };
            s.ValueChanged += (_, _) =>
            {
                if (setting) return;
                ShowTips();
                fxLater.Stop(); // the effects once the slider rests a moment (each move needn't be saved)
                fxLater.Start();
            };
            fx[i] = s;
            effects.Children.Add(Row(names[i], s, null));
        }
        body.Children.Add(effects);
        var reset = new Button { HorizontalAlignment = HorizontalAlignment.Left };
        var resetWords = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        resetWords.Children.Add(new FontIcon { Glyph = "", FontSize = 14 });
        resetWords.Children.Add(new TextBlock { Text = "Reset" });
        reset.Content = resetWords;
        ToolTipService.SetToolTip(reset, "Reset: every part at full, the effects off (the song as it is)");
        reset.Click += (_, _) => hub.Engine.ResetMix();
        body.Children.Add(reset);

        var ui = DispatcherQueue.GetForCurrentThread();
        fxLater = ui.CreateTimer();
        fxLater.Interval = TimeSpan.FromMilliseconds(120);
        fxLater.IsRepeating = false;
        fxLater.Tick += (_, _) => SendEffects();
        meters = ui.CreateTimer();
        meters.Interval = TimeSpan.FromMilliseconds(250);
        meters.Tick += (_, _) => Meters();
        Unloaded += (_, _) => Clocks(false);
        hub.Change += kind =>
        {
            if (kind == ChangeKind.Now) Render();
            // a four-part split's progress comes as a Splits change
            else if (kind == ChangeKind.Splits) RenderSplit();
        };
        Render();
    }

    /// <summary>A row: its title, its meter (parts only), its slider.</summary>
    private static Grid Row(string title, Slider slider, ProgressBar? meter)
    {
        var row = new Grid { ColumnSpacing = 10 };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(64) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var name = new TextBlock { Text = title, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], VerticalAlignment = VerticalAlignment.Center };
        row.Children.Add(name);
        if (meter != null)
        {
            Grid.SetColumn(meter, 1);
            row.Children.Add(meter);
        }
        Grid.SetColumn(slider, 2);
        row.Children.Add(slider);
        return row;
    }

    private void Clocks(bool on)
    {
        if (on && !meters.IsRunning) meters.Start();
        else if (!on && meters.IsRunning) meters.Stop();
    }

    /// <summary>Each part's loudness right now (before its slider: a part nearly empty in this song shows it).</summary>
    private void Meters()
    {
        if (parts.Count == 0 || XamlRoot == null) return;
        foreach (var level in hub.Engine.PartLevels())
            if (parts.TryGetValue(level.Name, out var p))
            {
                // in dB, as the Linux app's (core's mix::meter): the raw power hardly moved the bar
                p.Meter.Value = level.Meter;
                ToolTipService.SetToolTip(p.Meter, level.Tip);
            }
    }

    private void SendEffects() => hub.Engine.SetEffects(EffectText.LowCut(fx[0].Value / 100), EffectText.HighCut(fx[1].Value / 100), EffectText.Reverb(fx[2].Value / 100));

    private void ShowTips()
    {
        ToolTipService.SetToolTip(fx[0], EffectText.LowTip(EffectText.LowCut(fx[0].Value / 100)));
        ToolTipService.SetToolTip(fx[1], EffectText.HighTip(EffectText.HighCut(fx[1].Value / 100)));
        ToolTipService.SetToolTip(fx[2], EffectText.ReverbTip(EffectText.Reverb(fx[2].Value / 100)));
    }

    private void Render()
    {
        var now = hub.Engine.Now();
        var mix = now.Mix;
        setting = true;
        try
        {
            var names = string.Join("|", mix.Parts.Select(p => p.Name));
            if (names != shownParts)
            {
                shownParts = names;
                partRows.Children.Clear();
                parts.Clear();
                foreach (var part in mix.Parts)
                {
                    var name = part.Name;
                    var slider = new Slider { Minimum = 0, Maximum = 100, StepFrequency = 1, IsThumbToolTipEnabled = false, HorizontalAlignment = HorizontalAlignment.Stretch };
                    ToolTipService.SetToolTip(slider, $"{part.Title}: how loud (never louder than the song itself)");
                    slider.ValueChanged += (_, e) =>
                    {
                        if (!setting) hub.Engine.SetPart(name, e.NewValue / 100);
                    };
                    var meter = new ProgressBar { Minimum = 0, Maximum = 1, Width = 40, VerticalAlignment = VerticalAlignment.Center };
                    parts[name] = (slider, meter);
                    partRows.Children.Add(Row(part.Title, slider, meter));
                }
            }
            partRows.Visibility = mix.Parts.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
            RenderSplit();
            note.Text = mix.Note;
            note.Visibility = mix.Note.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
            foreach (var part in mix.Parts)
                if (parts.TryGetValue(part.Name, out var p)) p.Slider.Value = Math.Round(part.Gain * 100);
            var v = Array.FindIndex(Voices, x => x.Mode == mix.Voice);
            if (v >= 0 && !ReferenceEquals(voices.SelectedItem, voices.Items[v])) voices.SelectedItem = voices.Items[v];
            if (!fxLater.IsRunning)
            {
                fx[0].Value = Math.Round(EffectText.LowPos(mix.LowCut) * 100);
                fx[1].Value = Math.Round(EffectText.HighPos(mix.HighCut) * 100);
                fx[2].Value = Math.Round(EffectText.ReverbPos(mix.Reverb) * 100);
                ShowTips();
            }
        }
        finally
        {
            setting = false;
        }
    }

    /// <summary>The split line and the split choices for the song playing (engine's mixer_split).</summary>
    private void RenderSplit()
    {
        var s = hub.Engine.MixerSplit();
        splittingWords.Text = s.Line;
        splitBar.IsIndeterminate = s.Progress < 0;
        if (s.Progress >= 0) splitBar.Value = s.Progress * 100;
        turnOff.Visibility = s.TurnOff ? Visibility.Visible : Visibility.Collapsed;
        Motion.Fade(splitting, s.Line.Length > 0);
        var choices = string.Join("|", s.Items.Select(i => i.Label));
        if (choices != shownChoices)
        {
            shownChoices = choices;
            splitChoices.Children.Clear();
            foreach (var item in s.Items)
            {
                var action = item.Action;
                var words = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
                // the fast split: scissors; four parts: four squares
                words.Children.Add(new FontIcon { Glyph = action is MenuAction.SplitFour ? "\uE80A" : "\uE8C6", FontSize = 12 });
                words.Children.Add(new TextBlock { Text = item.Label, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] });
                var b = new Button { Content = words, Padding = new Thickness(10, 4, 10, 5) };
                ToolTipService.SetToolTip(b, action is MenuAction.SplitFour
                    ? "Drums, Bass, Other and Vocals for the Mixer (about 3 minutes, while it plays)"
                    : "Into Music and Vocals, a few seconds: then No vocals and Vocals only switch at once");
                b.Click += (_, _) =>
                {
                    if (hub.Engine.Now().Song is { } song) hub.Engine.DoMenu(song.Key, 0, action);
                };
                Motion.AddPressScale(b);
                splitChoices.Children.Add(b);
            }
        }
        splitChoices.Visibility = s.Items.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    /// <summary>Its pop-up opened or closed: the meters run while it's open.</summary>
    public void Shown(bool open) => Clocks(open);
}
