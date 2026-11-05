namespace RadianceMusic;

/// <summary>
/// The queue panel's drag to reorder: the ListView holds the songs after the one playing (queue
/// index `first` on); a row moved from `oldIndex` to `newIndex` there is the engine's
/// move_in_queue(from, to), `to` being the queue index it goes before (core's Queue::move_item).
/// </summary>
public static class QueueDrop
{
    public static (uint From, uint To)? Map(uint first, int oldIndex, int newIndex)
    {
        if (oldIndex < 0 || newIndex < 0 || oldIndex == newIndex) return null;
        var from = first + (uint)oldIndex;
        var to = newIndex > oldIndex ? first + (uint)newIndex + 1 : first + (uint)newIndex;
        return (from, to);
    }
}
