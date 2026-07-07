"""Roundtrip coverage for `baml_sdk.interfaces` — interface types (BEP-044).

Interfaces are BAML-side contracts, not serializable host SDK models:
codegen surfaces interface-typed boundary positions as `typing.Any`
(`client_codegen.rs`: `TirTy::Interface(..) => cg::Ty::BuiltinUnknown`).
The *values* crossing the boundary are always concrete class instances.

These tests pin:

  - return position: the host receives the concrete implementing class
    (for both the in-body `implements` form — `Square`/`Rect` — and the
    out-of-body `implements Shape for Circle` form)
  - parameter position: the host passes a concrete instance and the
    engine dispatches interface methods on it
  - round trips preserve the concrete class in bare, list, optional,
    and class-field positions
  - KNOWN GAPS (pinned so a fix flips these tests and forces an
    intentional update): no encode-time conformance check for interface
    params, and impl-method host bindings point at unregistered names

The python bridge encodes host instances by their runtime class, so all
of the above works here. The nodejs SDK currently diverges — its bridge
encodes by the declared codegen type, losing class identity in interface
positions; see roundtrip_interfaces.test.ts for the pinned node behavior.
"""

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_bridge.errors import BamlPanic
from baml_sdk.interfaces import (
    Circle,
    Rect,
    ShapeBox,
    Square,
    box_area,
    return_circle_as_shape,
    return_rect_as_shape,
    return_square_as_shape,
    round_trip_optional_shape,
    round_trip_shape,
    round_trip_shape_box,
    round_trip_shape_list,
    shape_area,
    shape_area_async,
    sum_areas,
)


# ── return position: host receives the concrete implementing class ──────


def test_return_square_as_shape():
    s = return_square_as_shape()
    assert isinstance(s, Square)
    assert s.side == 5


def test_return_rect_as_shape():
    s = return_rect_as_shape()
    assert isinstance(s, Rect)
    assert s == Rect(width=3, height=4)


def test_return_circle_as_shape():
    # Circle implements Shape via the out-of-body `implements ... for` form.
    s = return_circle_as_shape()
    assert isinstance(s, Circle)
    assert s.radius == 2


# ── parameter position: engine dispatches interface methods ─────────────


def test_shape_area_dispatches_on_square():
    assert shape_area(s=Square(side=5)) == 25


def test_shape_area_dispatches_on_rect():
    assert shape_area(s=Rect(width=3, height=4)) == 12


def test_shape_area_dispatches_on_out_of_body_impl():
    assert shape_area(s=Circle(radius=2)) == 12


async def test_shape_area_async_dispatches():
    assert await shape_area_async(s=Square(side=6)) == 36


def test_sum_areas_mixes_implementors():
    assert sum_areas(a=Square(side=2), b=Rect(width=3, height=4)) == 16


# ── round trips preserve the concrete class ─────────────────────────────


def test_round_trip_shape_preserves_concrete_class():
    r = round_trip_shape(s=Rect(width=2, height=3))
    assert isinstance(r, Rect)
    assert r == Rect(width=2, height=3)


def test_round_trip_shape_list_preserves_each_element():
    shapes = [Square(side=1), Rect(width=2, height=3), Circle(radius=4)]
    assert round_trip_shape_list(shapes=shapes) == shapes


def test_round_trip_optional_shape_none():
    assert round_trip_optional_shape(s=None) is None


def test_round_trip_optional_shape_value():
    r = round_trip_optional_shape(s=Circle(radius=1))
    assert isinstance(r, Circle)
    assert r.radius == 1


def test_round_trip_shape_box_field_position():
    r = round_trip_shape_box(b=ShapeBox(shape=Square(side=2)))
    assert isinstance(r, ShapeBox)
    assert isinstance(r.shape, Square)
    assert r.shape.side == 2


def test_box_area_dispatches_through_field():
    assert box_area(b=ShapeBox(shape=Rect(width=2, height=5))) == 10


# ── KNOWN GAPS — these pin *current* behavior, not desired behavior ─────


def test_non_implementor_panics_at_dispatch_not_encode():
    # There is no encode-time conformance check for interface-typed
    # params (the codegen type is `typing.Any`): a value whose class
    # does not implement Shape encodes fine and only fails inside the
    # VM when the virtual call cannot resolve. A future encode- or
    # call-time check would surface a TypeError instead — update this
    # pin when that lands. NB: BamlPanic derives BaseException.
    with pytest.raises(BamlPanic, match="could not resolve interface method"):
        shape_area(s=ShapeBox(shape=Square(side=1)))


def test_primitive_into_interface_param_panics_at_dispatch():
    with pytest.raises(BamlPanic, match="could not resolve interface method"):
        shape_area(s=42)


def test_impl_method_host_binding_is_not_callable():
    # sdkgen emits `area`/`area_async` bindings on implementing classes
    # (named `user.interfaces.Square.area`), but the engine registers
    # interface-impl methods under a synthetic `Shape$for$Square` name,
    # so calling the binding from the host panics with "Function not
    # found". Update this pin when the binding and registration agree.
    with pytest.raises(BamlPanic, match="Function not found"):
        Square(side=3).area()
