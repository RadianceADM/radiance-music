using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

public sealed partial class NowPlayingPage : Page
{
    /// <summary>Under these widths (px), as the GTK app: no lyrics column (1060), the cover above (860).</summary>
    private const double Snug = 1060, Narrow = 860;
    private EngineHub? hub;
    private readonly LyricFollower follower = new();
    private readonly List<TextBlock> lines = new();
    private string songKey = "";
    private string lyricsKey = "";

    public NowPlayingPage()
    {
        InitializeComponent();
        SizeChanged += (_, e) => Fit(e.NewSize.Width);
        LyricsScroll.SizeChanged += (_, e) => Lines.Padding = new Thickness(0, e.NewSize.Height / 2, 0, e.NewSize.Height / 2);
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub == null && e.Parameter is EngineHub h)
        {
            hub = h;
            hub.Change += kind =>
            {
                switch (kind)
                {
                    case ChangeKind.Now: RenderNow(); break;
                    case ChangeKind.Position: Light(hub.Engine.Now().Lit); break;
                    case ChangeKind.Lyrics or ChangeKind.Online: RenderLyrics(); break;
                }
            };
        }
        RenderNow();
        RenderLyrics();
    }

    private void Fit(double width)
    {
        var lyrics = width >= Snug;
        Right.Visibility = lyrics ? Visibility.Visible : Visibility.Collapsed;
        Left.HorizontalAlignment = lyrics ? HorizontalAlignment.Left : HorizontalAlignment.Center;
        Grid.SetColumnSpan(Left, lyrics ? 1 : 2);
        var side = width < Narrow ? Math.Max(160, Math.Min(340, width - 96)) : 340;
        Cover.Width = Cover.Height = Left.Width = side;
    }

    private void RenderNow()
    {
        if (hub == null) return;
        var now = hub.Engine.Now();
        var song = now.Song;
        if (song == null)
        {
            SongTitle.Text = "Nothing playing";
            Byline.Text = "";
            Quality.Visibility = Visibility.Collapsed;
            Cover.Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"];
            Back.SetArt(null, null);
            OverArt(false);
            songKey = "";
            return;
        }
        SongTitle.Text = song.Title;
        Byline.Text = song.Byline;
        // the GTK app's quality line (stage 1: lossless or compressed; bits and kHz come later)
        Quality.Visibility = Visibility.Visible;
        QualityText.Text = now.Lossless ? "Lossless · FLAC" : now.Kbps > 0 ? $"Compressed · Opus {now.Kbps} kbps" : "Compressed · Opus";
        if (song.Key != songKey)
        {
            songKey = song.Key;
            follower.Reset();
            _ = LoadArt(song.Art);
        }
        Light(now.Lit);
    }

    private async Task LoadArt(string source)
    {
        if (hub == null) return;
        var key = songKey;
        var cover = await ArtCache.LoadAsync(hub.Engine, source, 720);
        if (key != songKey) return;
        Cover.Background = cover != null ? new ImageBrush { ImageSource = cover, Stretch = Stretch.UniformToFill } : (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"];
        var files = await Task.Run(() => hub.Engine.Backdrop(source));
        if (key != songKey) return;
        Back.SetArt(files?.Wash, files?.Blob);
        OverArt(files != null);
    }

    /// <summary>Over the moving art the page is dark (its words stay readable); else Windows' own theme.</summary>
    private void OverArt(bool on)
    {
        Scrim.Visibility = on ? Visibility.Visible : Visibility.Collapsed;
        Root.RequestedTheme = on ? ElementTheme.Dark : ElementTheme.Default;
    }

    private void RenderLyrics()
    {
        if (hub == null) return;
        var l = hub.Engine.Lyrics();
        var key = songKey + "|" + l.Kind + "|" + l.Lines.Length + "|" + l.Plain.Length + "|" + hub.Engine.Online();
        if (key == lyricsKey) return;
        lyricsKey = key;
        Lines.Children.Clear();
        lines.Clear();
        follower.Reset();
        LyricsNote.Visibility = Visibility.Collapsed;
        switch (l.Kind)
        {
            case LyricsKind.Looking:
                Note("Looking for lyrics…");
                break;
            case LyricsKind.None:
                Note(hub.Engine.Online() ? "No lyrics found for this song." : "You're offline: the lyrics come when you're back online.");
                break;
            case LyricsKind.Plain:
                Lines.Children.Add(new TextBlock { Text = l.Plain, Style = (Style)Application.Current.Resources["LyricPlain"], Margin = new Thickness(0, 24, 0, 24) });
                break;
            case LyricsKind.Lines:
                foreach (var line in l.Lines)
                {
                    var t = new TextBlock { Text = line.Text, Style = (Style)Application.Current.Resources["Lyric"] };
                    t.OpacityTransition = new ScalarTransition { Duration = TimeSpan.FromMilliseconds(350) };
                    lines.Add(t);
                    Lines.Children.Add(t);
                }
                Light(hub.Engine.Now().Lit);
                break;
        }
    }

    private void Note(string text)
    {
        LyricsNote.Text = text;
        LyricsNote.Visibility = Visibility.Visible;
    }

    /// <summary>The lines sung now lit; the first of them brought to the middle (once, as it changes).</summary>
    private void Light(uint[] lit)
    {
        if (lines.Count == 0) return;
        for (var i = 0; i < lines.Count; i++) lines[i].Opacity = lit.Contains((uint)i) ? 1.0 : 0.34;
        if (follower.Next(lit) is int lead && lead < lines.Count && Right.Visibility == Visibility.Visible)
        {
            var line = lines[lead];
            var at = line.TransformToVisual(Lines).TransformPoint(new Windows.Foundation.Point(0, 0));
            var y = at.Y + line.ActualHeight / 2 - LyricsScroll.ViewportHeight / 2;
            LyricsScroll.ChangeView(null, Math.Max(0, y), null, false);
        }
    }
}
