using RadianceMusic;
using Xunit;

namespace RadianceMusic.Tests;

public enum Kind { Now, Position, Queue, Results, Home, Lyrics, Online, Problem }

public class CoalescerTests
{
    [Fact]
    public void two_marks_of_one_kind_post_once()
    {
        var c = new Coalescer<Kind>();
        Assert.True(c.Mark(Kind.Position));
        Assert.False(c.Mark(Kind.Position));
        Assert.Equal(new[] { Kind.Position }, c.Flush());
    }

    [Fact]
    public void a_flush_returns_every_kind_once_in_order()
    {
        var c = new Coalescer<Kind>();
        Assert.True(c.Mark(Kind.Results));
        Assert.False(c.Mark(Kind.Now));
        Assert.False(c.Mark(Kind.Results));
        Assert.Equal(new[] { Kind.Now, Kind.Results }, c.Flush());
        Assert.True(c.Mark(Kind.Now));
        Assert.Empty(new Coalescer<Kind>().Flush());
    }

    [Fact]
    public void marks_from_many_threads_lose_nothing()
    {
        var c = new Coalescer<Kind>();
        var seen = new HashSet<Kind>();
        var marked = new System.Collections.Concurrent.ConcurrentDictionary<Kind, bool>();
        var threads = Enumerable.Range(0, 8).Select(t => new Thread(() =>
        {
            for (var i = 0; i < 1000; i++)
            {
                var k = (Kind)((t + i) % 8);
                marked[k] = true;
                c.Mark(k);
                if (i % 97 == 0) lock (seen) foreach (var f in c.Flush()) seen.Add(f);
            }
        })).ToList();
        threads.ForEach(t => t.Start());
        threads.ForEach(t => t.Join());
        foreach (var f in c.Flush()) seen.Add(f);
        Assert.Equal(marked.Keys.OrderBy(k => k), seen.OrderBy(k => k));
    }
}
