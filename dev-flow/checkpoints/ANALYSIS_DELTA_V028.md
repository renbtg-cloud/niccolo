# v0.28 — Delta from frozen analysis input to current Situation

The delta read model compares a frozen analysis input with the current Situation under one repeatable-read transaction. It reports structural changes only: focus, source membership, and attributed human-context membership. A correction appears as removal of the old active contribution plus addition of its replacement; a retraction remains inspectable as a removed contribution whose current status is `RETRACTED`.

No delta is interpreted as evidence that an earlier conclusion is wrong or that a new analysis is required.
