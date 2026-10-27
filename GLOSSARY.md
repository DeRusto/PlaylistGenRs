# PlaylistGenRs

Builds one M3U playlist that interleaves episodes from many TV shows, so a folder of shows plays like a TV channel.

## Language

**Show**:
One TV series found in a scanned directory, made of seasons and episodes.
_Avoid_: Series, title

**Used show**:
A show checked for inclusion in the playlist; only used shows appear in the rotation.
_Avoid_: Selected show (selection is the highlighted table rows)

**Group**:
Two or more used shows that share a single slot and play back to back within it.
_Avoid_: Block, bundle

**Slot**:
One entry in the rotation: either a single ungrouped show or a whole group.
_Avoid_: Stack, bar, channel

**Turn**:
What a slot contributes to one round of the rotation: one episode, N episodes, or one season, depending on the interleave mode.
_Avoid_: Block (except for the N-episode mode itself), chunk

**Rotation**:
The repeating order in which slots take turns.
_Avoid_: Shuffle, schedule

**Balance**:
How evenly slots last in the rotation, measured in turns; a balanced rotation has every slot run out at about the same time.
_Avoid_: Weight
