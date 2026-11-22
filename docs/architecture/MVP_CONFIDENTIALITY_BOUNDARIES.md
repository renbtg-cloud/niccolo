# MVP confidentiality boundaries (AT-06 and AT-21)

The current operational compartment is the **workspace**, not the Situation, actor, modeled secrecy judgment or epistemic object. Every grant must be made by the actual authorization machinery. A registered actor or a reported authorization is not an authenticated principal or a permission grant.

## Supported operational partition

- Place information whose existence must be hidden from another investigator in a separately authorized workspace. A different Situation in the same workspace is not a privacy boundary. A caller with read access to that workspace can read its Situations.
- The authenticated principal's workspace memberships determine which boundaries they can cross. A principal authorized to more than one workspace may inspect each separately; credentials are not inherently bound to the workspace used when issuing them.
- Keep source material, reasoning objects, actor identities, decisions and every derived output within the applicable workspace. A shared name or externally meaningful reference does not identify a shared database object or establish cross-workspace dependence.
- The existing API and restrictive RLS policies enforce workspace membership independently of application-supplied workspace identifiers. No model judgment, alleged institutional authority, secrecy assessment or investigative importance changes those grants.

## Reviewing divergent findings and confidential changes

When two investigators have different information universes, their separate findings and grounds may remain independently represented. A reviewer explicitly authorized to both workspaces may inspect both separately and make a human comparison. The system does not merge their graphs, infer a conflict from shared text, or send a derived hint across the boundary. Access to both views is not authority to disclose either view to another audience.

An assessment revision can generate selective review requirements for explicit local dependencies. A protected revision does not suspend, disqualify, annotate or change an inaccessible workspace's findings, reasoning diagnostics or decisions. A reviewer who has access to both can investigate whether an independently authorized review or disclosure route exists. If there is no authorized route, cross-workspace review cannot be guaranteed. A generic alert is not presumed safe merely because it omits the grounds: the existence of a review need may itself be protected.

## Remaining implementation boundaries

The MVP does not yet provide overlapping object-level compartments, separate existence/content/reason permissions inside a workspace, automated cross-workspace review routing, purpose-restricted disclosure, a cross-workspace combined reasoning packet, or a guarantee that humans will respect an applicable disclosure restriction after receiving information. Do not represent these capabilities as implemented. Keep protected material out of shared workspaces unless each member is actually authorized to receive it.

The above boundary is limited to the supported authenticated product path and dedicated database API role. It is not a claim of protection against a privileged database operator, compromised process, previously exported data, or traffic/timing side channels. Revocation prevents future authorized reads under the revoked grant; it cannot erase knowledge already acquired.
