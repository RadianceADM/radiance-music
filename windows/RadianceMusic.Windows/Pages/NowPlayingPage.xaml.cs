using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

public sealed partial class NowPlayingPage : Page
{
    /// <summary>Under these widths of the page (px; the GTK app's window breakpoints, 1060 and 860, less
    /// the sidebar): no lyrics column, a smaller cover.</summary>
    private const double Snug = 780, Narrow = 600;
    private EngineHub? hub;
    private string songKey = "";

    public NowPlayingPage()
    {
        InitializeComponent();
        SizeChanged += (_, e) => Fit(e.NewSize.Width);
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub == null && e.Parameter is EngineHub h)
        {
            hub = h;
            // the lyrics column: word by word, growing toward the middle (Controls/LyricsColumn)
            Right.Children.Clear();
            Right.Children.Add(new Controls.LyricsColumn(hub, big: true));
            hub.Change += kind =>
            {
                if (kind == ChangeKind.Now) RenderNow();
            };
        }
        RenderNow();
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
            _ = LoadArt(song.Art);
        }
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
}
