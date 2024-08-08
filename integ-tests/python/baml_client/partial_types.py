###############################################################################
#
#  Welcome to Baml! To use this generated code, please run the following:
#
#  $ pip install baml
#
###############################################################################

# This file was generated by BAML: please do not edit it. Instead, edit the
# BAML files and re-generate this code.
#
# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off
import baml_py
from enum import Enum
from pydantic import BaseModel, ConfigDict
from typing import Dict, List, Optional, Union

from . import types

###############################################################################
#
#  These types are used for streaming, for when an instance of a type
#  is still being built up and any of its fields is not yet fully available.
#
###############################################################################


class Blah(BaseModel):
    
    
    prop4: Optional[str] = None

class ClassOptionalOutput(BaseModel):
    
    
    prop1: Optional[str] = None
    prop2: Optional[str] = None

class ClassOptionalOutput2(BaseModel):
    
    
    prop1: Optional[str] = None
    prop2: Optional[str] = None
    prop3: Optional["Blah"] = None

class ClassWithImage(BaseModel):
    
    
    myImage: Optional[baml_py.Image] = None
    param2: Optional[str] = None
    fake_image: Optional["FakeImage"] = None

class DummyOutput(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    nonce: Optional[str] = None
    nonce2: Optional[str] = None

class DynInputOutput(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    testKey: Optional[str] = None

class DynamicClassOne(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    

class DynamicClassTwo(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    hi: Optional[str] = None
    some_class: Optional["SomeClassNestedDynamic"] = None
    status: Optional[Union[types.DynEnumOne, str]] = None

class DynamicOutput(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    

class Education(BaseModel):
    
    
    institution: Optional[str] = None
    location: Optional[str] = None
    degree: Optional[str] = None
    major: List[Optional[str]]
    graduation_date: Optional[str] = None

class Email(BaseModel):
    
    
    subject: Optional[str] = None
    body: Optional[str] = None
    from_address: Optional[str] = None

class Event(BaseModel):
    
    
    title: Optional[str] = None
    date: Optional[str] = None
    location: Optional[str] = None
    description: Optional[str] = None

class FakeImage(BaseModel):
    
    
    url: Optional[str] = None

class InnerClass(BaseModel):
    
    
    prop1: Optional[str] = None
    prop2: Optional[str] = None
    inner: Optional["InnerClass2"] = None

class InnerClass2(BaseModel):
    
    
    prop2: Optional[int] = None
    prop3: Optional[float] = None

class NamedArgsSingleClass(BaseModel):
    
    
    key: Optional[str] = None
    key_two: Optional[bool] = None
    key_three: Optional[int] = None

class OptionalTest_Prop1(BaseModel):
    
    
    omega_a: Optional[str] = None
    omega_b: Optional[int] = None

class OptionalTest_ReturnType(BaseModel):
    
    
    omega_1: Optional["OptionalTest_Prop1"] = None
    omega_2: Optional[str] = None
    omega_3: List[Optional[types.OptionalTest_CategoryType]]

class OrderInfo(BaseModel):
    
    
    order_status: Optional[types.OrderStatus] = None
    tracking_number: Optional[str] = None
    estimated_arrival_date: Optional[str] = None

class Person(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    name: Optional[str] = None
    hair_color: Optional[Union[types.Color, str]] = None

class Quantity(BaseModel):
    
    
    amount: Optional[Union[Optional[int], Optional[float]]] = None
    unit: Optional[str] = None

class RaysData(BaseModel):
    
    
    dataType: Optional[types.DataType] = None
    value: Optional[Union["Resume", "Event"]] = None

class ReceiptInfo(BaseModel):
    
    
    items: List["ReceiptItem"]
    total_cost: Optional[float] = None

class ReceiptItem(BaseModel):
    
    
    name: Optional[str] = None
    description: Optional[str] = None
    quantity: Optional[int] = None
    price: Optional[float] = None

class Recipe(BaseModel):
    
    
    ingredients: Dict[str, Optional["Quantity"]]

class Resume(BaseModel):
    
    
    name: Optional[str] = None
    email: Optional[str] = None
    phone: Optional[str] = None
    experience: List["Education"]
    education: List[Optional[str]]
    skills: List[Optional[str]]

class SearchParams(BaseModel):
    
    
    dateRange: Optional[int] = None
    location: List[Optional[str]]
    jobTitle: Optional["WithReasoning"] = None
    company: Optional["WithReasoning"] = None
    description: List["WithReasoning"]
    tags: List[Optional[Union[Optional[types.Tag], Optional[str]]]]

class SomeClassNestedDynamic(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    hi: Optional[str] = None

class StringToClassEntry(BaseModel):
    
    
    word: Optional[str] = None

class TestClassAlias(BaseModel):
    
    
    key: Optional[str] = None
    key2: Optional[str] = None
    key3: Optional[str] = None
    key4: Optional[str] = None
    key5: Optional[str] = None

class TestClassNested(BaseModel):
    
    
    prop1: Optional[str] = None
    prop2: Optional["InnerClass"] = None

class TestClassWithEnum(BaseModel):
    
    
    prop1: Optional[str] = None
    prop2: Optional[types.EnumInClass] = None

class TestOutputClass(BaseModel):
    
    
    prop1: Optional[str] = None
    prop2: Optional[int] = None

class UnionTest_ReturnType(BaseModel):
    
    
    prop1: Optional[Union[Optional[str], Optional[bool]]] = None
    prop2: List[Optional[Union[Optional[float], Optional[bool]]]]
    prop3: Optional[Union[List[Optional[bool]], List[Optional[int]]]] = None

class WithReasoning(BaseModel):
    
    
    value: Optional[str] = None
    reasoning: Optional[str] = None
