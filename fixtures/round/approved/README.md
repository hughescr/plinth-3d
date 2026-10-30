# Approved tilted Round bases (stage 1)

**APPROVED by the owner 2026-10-30 (65°).** These are the approved copies named in PRD FR-11a and dev-loop design section 9 and step 10.

## Approval record

- **Date:** 2026-10-30
- **Approved by:** Craig, the owner
- **Angle:** 65 degrees from flat (the rotation is given under "Rotation" below)
- **Owner's words:** "The tilted STL round bases look plausible, angles look OK."
- **Scope:** the three `.keepup.stl` files and the nine pictures in `pictures/`, as they are in this folder. These are the pictures he approved, so they were not re-rendered. The labels on the PNGs still read PROPOSED. That label predates the approval and is left as it is.
- **Not explicitly covered:** his words are about the look and the angle. They do not confirm which side is the underside; see Doubts below.

## What these files are

Rigid copies of the three originals in `../original/`, turned so that the base disc is inclined 65 degrees from flat. The build plate is flat with Z up. The underside (the lip-and-rings side) is on the lower side of the inclined disc and faces down toward the plate. The decorated top faces up and away. The file names end `.keepup.stl`, which is the keep-up marker by name pattern (PRD FR-6).

## Rotation (same for all three)

- Axis: the X axis, through the model origin (0, 0, 0). Right-handed.
- Angle: +115 degrees. A point (x, y, z) goes to (x, y cos115 - z sin115, y sin115 + z cos115).
- Effect: the underside normal (0, 0, +1) becomes (0, -0.9063, -0.4226): it points down and toward -Y. The top normal becomes (0, +0.9063, +0.4226). The disc axis is 65 degrees from the Z axis; the disc is 65 degrees from flat.
- Nothing else changed: no scaling, no translation (the model is not lowered to the plate; ChituBox or plinth places it), same triangle order, same header and attribute bytes. Vertices and normals were rotated in double precision and written as f32.
- To pick another angle, rotate the original about X by (180 - angle) degrees.

## Files and hashes

| Derived file | Size | SHA-256 | Original SHA-256 |
|---|---|---|---|
| `Round 25.keepup.stl` | 149884 | `853709e4063de2e3c9dc7a646f13d5ede5a2f26c4a4372ab220cfe153480750a` | `c293b0b6a397cc62a25445a1281ece7dbfb6c6ae07c68d0f3f191e10a35b2b17` |
| `Round 50.keepup.stl` | 300784 | `3731e2955b804733f2deba5a1fb14ea5a67be5fa757686a29c379dc60e3c383b` | `f2de1f789a9ed2bfbf2dafb65b8e3f44dcd391de7c0da3563dde8ba97d13d104` |
| `Round 100.keepup.stl` | 98384 | `5c52b320fdf48e1ac0ddade1ff0afcbfd11554ee86334ce8ac21e3a21ced6adf` | `cb6b4b0687f1e8f02a8fc59dc1391931e615417c42a23a398ad8824c65d04f82` |

## Measured checks

Measured by a second small program that reads the original and derived binary STL files and measures them (double precision). All numbers are from the written files.

| Check | Round 25 | Round 50 | Round 100 |
|---|---|---|---|
| Triangles, original / derived | 2996 / 2996 | 6014 / 6014 | 1966 / 1966 |
| Volume, original (mm3) | 851.898020 | 2804.511605 | 9491.818272 |
| Volume, derived (mm3) | 851.898021 | 2804.511616 | 9491.818960 |
| Volume relative difference | 1.2e-9 | 4.1e-9 | 7.2e-8 |
| Largest edge-length change (mm) | 7.6e-7 | 1.9e-6 | 2.3e-6 |
| Largest vertex-distance-from-origin change (mm) | 4.5e-7 | 9.8e-7 | 2.0e-6 |
| Disc normal angle from Z axis (dominant face normal) | 65.000 deg | 65.000 deg | 65.000 deg |
| Area-weighted mean normal z, faces that were +Z (underside) | -0.4226 (faces down) | -0.4226 | -0.4226 |
| Area-weighted mean normal z, faces that were -Z (top) | +0.4226 (faces up) | +0.4226 | +0.4226 |
| Stored normal vs triangle winding, worst (1 - cos) | 5.7e-8 | 5.9e-8 | 5.0e-8 |

The underside is also identified by shape, independent of the original: of the two opposite flat sides, the one with two flat levels (the recessed floor and the lip/ring ends; measured at -2.50 and 0.00 mm along the axis) is the one whose normals have z = -0.4226; the plain side has one level (3.50 mm) and z = +0.4226.

The 65 degree tolerance asked for was +-0.5 degrees; the measured error is below 0.001 degrees.

## Pictures

`pictures/` holds nine labelled PNGs, three per base: (1) side view with the 65 degree angle against the plate, (2) looking at the underside from the low side, (3) close view of the rim where the disc meets the plate. Orange = underside, blue-grey = top and rim wall, grid squares 10 mm. The model is lowered onto the plate in the pictures only; the STL files are not.

## Doubts

- Which side is the underside is taken from the geometry (the lip-and-rings side) and the PRD/LESSONS note that the drawn files have the glue face up. The "decorated" top face measures as one plain flat plane in these STLs (no relief on it); the owner should confirm in the pictures that the orange side is the one he calls the underside.
- The direction of the lean (toward -Y) is arbitrary; any rotation about Z gives the same print.
- The underside shows raised rings (1, 3 and 5 of them), not long ribs. The PRD calls them stiffening ribs.
