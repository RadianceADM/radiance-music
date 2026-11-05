using Microsoft.UI.Xaml.Controls;

namespace RadianceMusic.Pages;

public sealed partial class NowPlayingPage : Page
{
    public NowPlayingPage()
    {
        InitializeComponent();
        Title.Text = "Now Playing";
    }
}
