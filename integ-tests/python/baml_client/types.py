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
from typing import List, Optional, Union


class Category(str, Enum):
    
    Refund = "Refund"
    CancelOrder = "CancelOrder"
    TechnicalSupport = "TechnicalSupport"
    AccountIssue = "AccountIssue"
    Question = "Question"

class Category2(str, Enum):
    
    Refund = "Refund"
    CancelOrder = "CancelOrder"
    TechnicalSupport = "TechnicalSupport"
    AccountIssue = "AccountIssue"
    Question = "Question"

class Category3(str, Enum):
    
    Refund = "Refund"
    CancelOrder = "CancelOrder"
    TechnicalSupport = "TechnicalSupport"
    AccountIssue = "AccountIssue"
    Question = "Question"

class Color(str, Enum):
    
    RED = "RED"
    BLUE = "BLUE"
    GREEN = "GREEN"
    YELLOW = "YELLOW"
    BLACK = "BLACK"
    WHITE = "WHITE"

class DataType(str, Enum):
    
    Resume = "Resume"
    Event = "Event"

class DynEnumOne(str, Enum):
    pass

class DynEnumTwo(str, Enum):
    pass

class EnumInClass(str, Enum):
    
    ONE = "ONE"
    TWO = "TWO"

class EnumOutput(str, Enum):
    
    ONE = "ONE"
    TWO = "TWO"
    THREE = "THREE"

class Gender(str, Enum):
    
    Male = "Male"
    Female = "Female"
    Other = "Other"

class Hobby(str, Enum):
    
    SPORTS = "SPORTS"
    MUSIC = "MUSIC"
    READING = "READING"

class NamedArgsSingleEnum(str, Enum):
    
    ONE = "ONE"
    TWO = "TWO"

class NamedArgsSingleEnumList(str, Enum):
    
    ONE = "ONE"
    TWO = "TWO"

class OptionalTest_CategoryType(str, Enum):
    
    Aleph = "Aleph"
    Beta = "Beta"
    Gamma = "Gamma"

class OrderStatus(str, Enum):
    
    ORDERED = "ORDERED"
    SHIPPED = "SHIPPED"
    DELIVERED = "DELIVERED"
    CANCELLED = "CANCELLED"

class PartyOfficial(str, Enum):
    
    Labour = "Labour"
    Conservative = "Conservative"
    Liberal_Democrat = "Liberal_Democrat"
    Green_Party = "Green_Party"
    Reform_Party = "Reform_Party"
    Labour_Co_op = "Labour_Co_op"
    Social_Democratic = "Social_Democratic"
    Independent = "Independent"
    Scottish_National_Party = "Scottish_National_Party"

class Region(str, Enum):
    
    England = "England"
    London = "London"
    North_East = "North_East"
    North_West = "North_West"
    Yorkshire = "Yorkshire"
    East_Midlands = "East_Midlands"
    West_Midlands = "West_Midlands"
    South_East = "South_East"
    East_of_England = "East_of_England"
    South_West = "South_West"
    Scotland = "Scotland"
    Wales = "Wales"
    Northern_Ireland = "Northern_Ireland"

class Tag(str, Enum):
    
    Security = "Security"
    AI = "AI"
    Blockchain = "Blockchain"

class TestEnum(str, Enum):
    
    A = "A"
    B = "B"
    C = "C"
    D = "D"
    E = "E"
    F = "F"
    G = "G"

class Actor(BaseModel):
    
    
    person: Optional[str] = None
    party: "Party"
    region: Optional["Region"] = None
    gender: Optional["Gender"] = None

class ActorSubject(BaseModel):
    
    
    actors: List["Actor"]
    subject: List[str]
    dates: List[str]

class Blah(BaseModel):
    
    
    prop4: Optional[str] = None

class ClassOptionalOutput(BaseModel):
    
    
    prop1: str
    prop2: str

class ClassOptionalOutput2(BaseModel):
    
    
    prop1: Optional[str] = None
    prop2: Optional[str] = None
    prop3: Optional["Blah"] = None

class ClassWithImage(BaseModel):
    
    
    myImage: baml_py.Image
    param2: str
    fake_image: "FakeImage"

class DynInputOutput(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    testKey: str

class DynamicClassOne(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    

class DynamicClassTwo(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    hi: str
    some_class: "SomeClassNestedDynamic"
    status: Union["DynEnumOne", str]

class DynamicOutput(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    

class Education(BaseModel):
    
    
    institution: str
    location: str
    degree: str
    major: List[str]
    graduation_date: Optional[str] = None

class Email(BaseModel):
    
    
    subject: str
    body: str
    from_address: str

class Event(BaseModel):
    
    
    title: str
    date: str
    location: str
    description: str

class FakeImage(BaseModel):
    
    
    url: str

class InnerClass(BaseModel):
    
    
    prop1: str
    prop2: str
    inner: "InnerClass2"

class InnerClass2(BaseModel):
    
    
    prop2: int
    prop3: float

class NamedArgsSingleClass(BaseModel):
    
    
    key: str
    key_two: bool
    key_three: int

class OptionalTest_Prop1(BaseModel):
    
    
    omega_a: str
    omega_b: int

class OptionalTest_ReturnType(BaseModel):
    
    
    omega_1: Optional["OptionalTest_Prop1"] = None
    omega_2: Optional[str] = None
    omega_3: List[Optional["OptionalTest_CategoryType"]]

class OrderInfo(BaseModel):
    
    
    order_status: "OrderStatus"
    tracking_number: Optional[str] = None
    estimated_arrival_date: Optional[str] = None

class Party(BaseModel):
    
    
    name: str
    official: Optional["PartyOfficial"] = None

class Person(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    name: Optional[str] = None
    hair_color: Optional[Union["Color", str]] = None

class RaysData(BaseModel):
    
    
    dataType: "DataType"
    value: Union["Resume", "Event"]

class ReceiptInfo(BaseModel):
    
    
    items: List["ReceiptItem"]
    total_cost: Optional[float] = None

class ReceiptItem(BaseModel):
    
    
    name: str
    description: Optional[str] = None
    quantity: int
    price: float

class Resume(BaseModel):
    
    
    name: str
    email: str
    phone: str
    experience: List["Education"]
    education: List[str]
    skills: List[str]

class SearchParams(BaseModel):
    
    
    dateRange: Optional[int] = None
    location: List[str]
    jobTitle: Optional["WithReasoning"] = None
    company: Optional["WithReasoning"] = None
    description: List["WithReasoning"]
    tags: List[Union["Tag", str]]

class SomeClassNestedDynamic(BaseModel):
    
    model_config = ConfigDict(extra='allow')
    
    hi: str

class TestClassAlias(BaseModel):
    
    
    key: str
    key2: str
    key3: str
    key4: str
    key5: str

class TestClassNested(BaseModel):
    
    
    prop1: str
    prop2: "InnerClass"

class TestClassWithEnum(BaseModel):
    
    
    prop1: str
    prop2: "EnumInClass"

class TestOutputClass(BaseModel):
    
    
    prop1: str
    prop2: int

class UnionTest_ReturnType(BaseModel):
    
    
    prop1: Union[str, bool]
    prop2: List[Union[float, bool]]
    prop3: Union[List[bool], List[int]]

class WithReasoning(BaseModel):
    
    
    value: str
    reasoning: str
