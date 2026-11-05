using Microsoft.UI.Xaml.Controls;

namespace RadianceMusic.Pages;

public sealed partial class SearchPage : Page
{
    public SearchPage()
    {
        InitializeComponent();
        Title.Text = "Search";
    }

    public void Search(string q) => Title.Text = "Search: " + q;
}
