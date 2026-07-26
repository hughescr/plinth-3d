"""Logical-demand provenance for shared specialized physical contacts.

Minimum and slice-birth requirements are logical obligations.  Several of
those obligations may be discharged by one physical contact when one of the
authoritative nominated seats satisfies every obligation.  This module keeps
that distinction explicit: ``specialized_demands`` is the immutable-in-spirit
logical sidecar, while the containing contact dict is the one physical seat
consumed by construction and emission.
"""
import copy

import numpy as np

from . import perception


def is_specialized(contact):
    return bool(
        contact.get("minimum") or contact.get("island_birth")
        or contact.get("specialized_demands"))


def _copy_array(value):
    return np.asarray(value, dtype=np.float64).copy()


def _demand_from_contact(contact):
    demand = dict(
        island_id=int(contact["island_id"]),
        # Birth-only contacts carry ``minimum=True`` at the physical layer so
        # the router treats them as mandatory specialized terminals.  A
        # logical minimum obligation exists only when the authoritative
        # birth-region payload exists.
        minimum=bool(contact.get("minimum") and "region_pts" in contact),
        island_birth=bool(contact.get("island_birth")),
        coverage_face_id=int(contact["coverage_face_id"]),
        coverage_component_id=int(contact.get(
            "coverage_component_id", -1)),
        candidate_position=_copy_array(contact["position"]),
        candidate_normal=_copy_array(contact["normal"]),
        area_mm2=float(contact.get("area_mm2", 0.0)),
    )
    if contact.get("minimum") and "region_pts" in contact:
        demand["region_pts"] = _copy_array(contact["region_pts"])
    if contact.get("island_birth"):
        demand["birth_loop"] = _copy_array(contact["birth_loop"])
        demand["birth_z"] = float(contact["birth_z"])
    return demand


def logical_demands(contact):
    """Return defensive logical-demand copies in canonical stored order."""
    stored = contact.get("specialized_demands")
    if stored is None:
        return [_demand_from_contact(contact)] if is_specialized(contact) else []
    return [copy.deepcopy(row) for row in stored]


def owner_island_ids(contact):
    if not is_specialized(contact):
        return (int(contact.get("island_id", -1)),)
    stored = contact.get("specialized_demands")
    if stored is None:
        return (int(contact["island_id"]),)
    owners = {int(row["island_id"]) for row in stored}
    if contact.get("shared_regular_contact"):
        owners.add(int(contact["island_id"]))
    return tuple(sorted(owners))


def coverage_routes(contact):
    """Yield every authoritative ``(island, face, component)`` route."""
    if not is_specialized(contact):
        face_id = contact.get("face_id")
        if face_id is not None:
            yield (
                int(contact.get("island_id", -1)),
                int(face_id),
                None,
            )
        return
    if contact.get("shared_regular_contact"):
        yield (
            int(contact["island_id"]),
            int(contact["face_id"]),
            None,
        )
    for demand in logical_demands(contact):
        yield (
            int(demand["island_id"]),
            int(demand["coverage_face_id"]),
            int(demand["coverage_component_id"]),
        )


def _array_key(value):
    arr = np.asarray(value, dtype=np.float64)
    return (arr.shape, arr.tobytes(order="C"))


def demand_key(demand):
    return (
        int(demand["coverage_component_id"]),
        tuple(float(x) for x in demand["candidate_position"]),
        int(demand["coverage_face_id"]),
        int(demand["island_id"]),
        bool(demand.get("minimum")),
        bool(demand.get("island_birth")),
        _array_key(demand["region_pts"])
        if "region_pts" in demand else (),
        float(demand.get("birth_z", 0.0)),
        _array_key(demand["birth_loop"])
        if "birth_loop" in demand else (),
    )


def candidate_key(demand):
    return (
        int(demand["coverage_component_id"]),
        tuple(float(x) for x in demand["candidate_position"]),
        int(demand["coverage_face_id"]),
        tuple(float(x) for x in demand["candidate_normal"]),
    )


def candidate_satisfies_demand(position, demand, cfg):
    position = np.asarray(position, dtype=np.float64)
    if demand.get("minimum"):
        region = np.asarray(demand.get("region_pts", ()), dtype=np.float64)
        if (not len(region)
                or float(np.linalg.norm(region - position, axis=1).min())
                > perception.ISLAND_ATTACH_REACH_MM):
            return False
    if demand.get("island_birth"):
        z_tol = float(cfg.minima_connect_layers) * float(cfg.layer_height_mm)
        if abs(float(position[2]) - float(demand["birth_z"])) > z_tol:
            return False
        ok, _distance = perception.island_birth_xy_ok(
            float(position[0]), float(position[1]),
            np.asarray(demand["birth_loop"], dtype=np.float64),
            perception.ISLAND_ATTACH_REACH_MM)
        if not ok:
            return False
    return True


def compatible_representatives(left, right, cfg):
    """Canonical authoritative candidates able to satisfy both demand sets."""
    demands = logical_demands(left) + logical_demands(right)
    components = {int(row["coverage_component_id"]) for row in demands}
    if len(components) != 1:
        return []
    candidates = {}
    for demand in demands:
        candidates.setdefault(candidate_key(demand), demand)
    out = []
    for _key, demand in sorted(candidates.items()):
        position = np.asarray(demand["candidate_position"], dtype=np.float64)
        locked_positions = [
            np.asarray(contact["position"], dtype=np.float64)
            for contact in (left, right)
            if contact.get("shared_regular_contact")
        ]
        if any(not np.array_equal(position, locked)
               for locked in locked_positions):
            continue
        if all(candidate_satisfies_demand(position, row, cfg)
               for row in demands):
            out.append(demand)
    return out


def merged_contact(left, right, representative, islands, mesh):
    """Build one physical contact carrying the union of logical demands."""
    demands = logical_demands(left) + logical_demands(right)
    unique = {}
    for demand in demands:
        unique.setdefault(demand_key(demand), demand)
    demands = [unique[key] for key in sorted(unique)]
    representative_key = candidate_key(representative)
    representative = next(
        row for row in demands if candidate_key(row) == representative_key)

    regular = next(
        (contact for contact in (left, right)
         if contact.get("shared_regular_contact")),
        None)
    contact = copy.deepcopy(regular if regular is not None else left)
    for key in (
            "region_pts", "birth_loop", "birth_z",
            "specialized_demands", "owner_island_ids"):
        contact.pop(key, None)
    contact.update(
        minimum=any(
            bool(row.get("minimum") or row.get("island_birth"))
            for row in demands),
        island_birth=any(bool(row.get("island_birth")) for row in demands),
        specialized_demands=tuple(copy.deepcopy(demands)),
        owner_island_ids=tuple(sorted({
            int(row["island_id"]) for row in demands
        })),
        shared_specialized_contact=True,
    )
    if regular is None:
        contact.update(
            position=_copy_array(representative["candidate_position"]),
            normal=_copy_array(representative["candidate_normal"]),
            island_id=int(representative["island_id"]),
            coverage_face_id=int(representative["coverage_face_id"]),
            coverage_component_id=int(
                representative["coverage_component_id"]),
        )
    owner_faces = np.unique(np.concatenate([
        np.asarray(islands[iid], dtype=np.int64)
        for iid in contact["owner_island_ids"]
    ]))
    contact["area_mm2"] = float(
        np.asarray(mesh.area_faces, dtype=np.float64)[owner_faces].sum())

    # Keep the representative payload for legacy single-site relocation code.
    if representative.get("minimum") and "region_pts" in representative:
        contact["region_pts"] = _copy_array(representative["region_pts"])
    if representative.get("island_birth"):
        contact["birth_loop"] = _copy_array(representative["birth_loop"])
        contact["birth_z"] = float(representative["birth_z"])
    return contact


def attach_to_exact_regular(regular, specialized, islands, mesh):
    """Credit specialized obligations to an identical authoritative A2 seat."""
    if is_specialized(regular) or not is_specialized(specialized):
        raise ValueError("expected regular then specialized contact")
    if not np.array_equal(
            np.asarray(regular["position"], dtype=np.float64),
            np.asarray(specialized["position"], dtype=np.float64)):
        raise ValueError("regular/specialized seats are not exact duplicates")
    demands = logical_demands(specialized)
    contact = copy.deepcopy(regular)
    contact.update(
        minimum=any(
            bool(row.get("minimum") or row.get("island_birth"))
            for row in demands),
        island_birth=any(bool(row.get("island_birth")) for row in demands),
        specialized_demands=tuple(copy.deepcopy(sorted(
            demands, key=demand_key))),
        owner_island_ids=tuple(sorted(
            {int(regular["island_id"])}
            | {int(row["island_id"]) for row in demands})),
        shared_regular_contact=True,
        shared_specialized_contact=True,
        coverage_face_id=int(specialized["coverage_face_id"]),
        coverage_component_id=int(
            specialized["coverage_component_id"]),
    )
    if "region_pts" in specialized:
        contact["region_pts"] = _copy_array(specialized["region_pts"])
    if specialized.get("island_birth"):
        contact["birth_loop"] = _copy_array(specialized["birth_loop"])
        contact["birth_z"] = float(specialized["birth_z"])
    owner_faces = np.unique(np.concatenate([
        np.asarray(islands[iid], dtype=np.int64)
        for iid in contact["owner_island_ids"]
    ]))
    contact["area_mm2"] = float(
        np.asarray(mesh.area_faces, dtype=np.float64)[owner_faces].sum())
    return contact


def exact_duplicate_seat_pairs(contacts):
    """All exact duplicate physical seats, independent of logical ownership."""
    seen = {}
    out = []
    for index, contact in enumerate(contacts):
        key = tuple(float(x) for x in np.asarray(
            contact["position"], dtype=np.float64))
        prior = seen.get(key)
        if prior is None:
            seen[key] = int(index)
        else:
            out.append((int(prior), int(index)))
    return out


def shared_contact_payload_violations(contact, cfg):
    if not contact.get("specialized_demands"):
        return []
    demands = logical_demands(contact)
    out = []
    keys = [demand_key(row) for row in demands]
    if keys != sorted(keys) or len(keys) != len(set(keys)):
        out.append("noncanonical-logical-demands")
    expected = {int(row["island_id"]) for row in demands}
    if contact.get("shared_regular_contact"):
        expected.add(int(contact["island_id"]))
    expected_owners = tuple(sorted(expected))
    if tuple(contact.get("owner_island_ids", ())) != expected_owners:
        out.append("owner-island-ids-mismatch")
    position = np.asarray(contact["position"], dtype=np.float64)
    representative_keys = {
        candidate_key(row) for row in demands
        if np.array_equal(
            np.asarray(row["candidate_position"], dtype=np.float64),
            position)
        and np.array_equal(
            np.asarray(row["candidate_normal"], dtype=np.float64),
            np.asarray(contact["normal"], dtype=np.float64))
        and int(row["coverage_face_id"])
        == int(contact["coverage_face_id"])
    }
    if not representative_keys and not contact.get("shared_regular_contact"):
        out.append("non-authoritative-representative")
    for demand_idx, demand in enumerate(demands):
        if not candidate_satisfies_demand(position, demand, cfg):
            out.append(f"unsatisfied-logical-demand:{demand_idx}")
    return out
