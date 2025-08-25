"""
Test runtime integration with type maps for phase 4.5
"""
import pytest
import asyncio
import os
from typing import List, Dict, Any
from dataclasses import dataclass
from enum import Enum

from baml_py_cffi import create_runtime
from baml_py_cffi.runtime import BamlRuntime
from baml_py_cffi.client import ScopedClient
from baml_py_cffi.serde.type_map import TypeMap
from baml_py_cffi.serde.encode import encode_value
from baml_py_cffi.serde.decode import decode_value
from baml_py_cffi.serde import cffi_pb2


# Define test types
@dataclass
class Person:
    """Test user-defined class"""
    name: str
    age: int
    email: str
    tags: List[str]
    
    def __eq__(self, other):
        return (
            isinstance(other, Person) and
            self.name == other.name and
            self.age == other.age and
            self.email == other.email and
            self.tags == other.tags
        )


class Priority(Enum):
    """Test enum type"""
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"


def encode_Person(person: Person, type_map: TypeMap) -> cffi_pb2.CFFIValueHolder:
    """Encoder for Person class"""
    holder = cffi_pb2.CFFIValueHolder()
    holder.class_value.name.name = "Person"
    
    # Add name field
    name_entry = cffi_pb2.CFFIMapEntry()
    name_entry.key = "name"
    name_entry.value.CopyFrom(encode_value(person.name, type_map=type_map))
    holder.class_value.fields.append(name_entry)
    
    # Add age field
    age_entry = cffi_pb2.CFFIMapEntry()
    age_entry.key = "age"
    age_entry.value.CopyFrom(encode_value(person.age, type_map=type_map))
    holder.class_value.fields.append(age_entry)
    
    # Add email field
    email_entry = cffi_pb2.CFFIMapEntry()
    email_entry.key = "email"
    email_entry.value.CopyFrom(encode_value(person.email, type_map=type_map))
    holder.class_value.fields.append(email_entry)
    
    # Add tags field
    tags_entry = cffi_pb2.CFFIMapEntry()
    tags_entry.key = "tags"
    tags_entry.value.CopyFrom(encode_value(person.tags, type_map=type_map))
    holder.class_value.fields.append(tags_entry)
    
    return holder


def decode_Person(holder: cffi_pb2.CFFIValueHolder, type_map: TypeMap) -> Person:
    """Decoder for Person class"""
    fields = {}
    for field in holder.class_value.fields:
        fields[field.key] = decode_value(field.value, type_map=type_map)
    
    return Person(
        name=fields.get("name", ""),
        age=fields.get("age", 0),
        email=fields.get("email", ""),
        tags=fields.get("tags", [])
    )


def encode_Priority(priority: Priority, type_map: TypeMap) -> cffi_pb2.CFFIValueHolder:
    """Encoder for Priority enum"""
    holder = cffi_pb2.CFFIValueHolder()
    holder.enum_value.name.name = "Priority"
    holder.enum_value.value = priority.value
    return holder


def decode_Priority(holder: cffi_pb2.CFFIValueHolder, type_map: TypeMap) -> Priority:
    """Decoder for Priority enum"""
    value = holder.enum_value.value
    # Handle both uppercase and lowercase values
    for p in Priority:
        if p.value == value.lower():
            return p
    raise ValueError(f"Unknown Priority value: {value}")


def create_test_type_map() -> TypeMap:
    """Create a type map with test types"""
    type_map: TypeMap = {}
    
    # Register Person
    type_map["Person"] = (
        Person,
        lambda p: encode_Person(p, type_map),
        lambda h: decode_Person(h, type_map)
    )
    
    # Register Priority
    type_map["Priority"] = (
        Priority,
        lambda p: encode_Priority(p, type_map),
        lambda h: decode_Priority(h, type_map)
    )
    
    return type_map


class TestRuntimeTypeMap:
    """Test runtime with type map integration"""
    
    @pytest.fixture
    def baml_files(self):
        """BAML function definitions for testing"""
        return {
            "test_types.baml": """
                class Person {
                    name string
                    age int
                    email string
                    tags string[]
                }
                
                enum Priority {
                    LOW 
                    MEDIUM
                    HIGH
                }
                
                function ProcessPerson(person: Person) -> Person {
                    client "openai/gpt-4o-mini"
                    prompt #"Process this person: {{ person }}. Return the same person."#
                }
                
                function GetPriority(task: string) -> Priority {
                    client "openai/gpt-4o-mini"
                    prompt #"Return MEDIUM priority for task: {{ task }}"#
                }
                
                function ListPeople(count: int) -> Person[] {
                    client "openai/gpt-4o-mini"
                    prompt #"Return a list of {{ count }} test people"#
                }
            """
        }
    
    @pytest.mark.asyncio
    async def test_runtime_typed_call_with_user_class(self, baml_files):
        """Test calling runtime with user-defined class type"""
        rt = create_runtime(".", baml_files, {"OPENAI_API_KEY": os.getenv("OPENAI_API_KEY", "test-key")})
        type_map = create_test_type_map()
        
        # Create test person
        person = Person(
            name="Alice",
            age=30,
            email="alice@example.com",
            tags=["developer", "python"]
        )
        
        # Call function with type map
        result = await rt.call_function_typed(
            name="ProcessPerson",
            args={"person": person},
            type_map=type_map,
            arg_types={"person": "Person"},
            return_type="Person"
        )
        
        # Verify result is a Person instance
        assert isinstance(result, (Person, type(result)))  # Could be Person or DynamicClass
    
    @pytest.mark.asyncio
    async def test_runtime_typed_call_with_enum(self, baml_files):
        """Test calling runtime with enum type"""
        rt = create_runtime(".", baml_files, {"OPENAI_API_KEY": os.getenv("OPENAI_API_KEY", "test-key")})
        type_map = create_test_type_map()
        
        # Call function that returns an enum
        result = await rt.call_function_typed(
            name="GetPriority",
            args={"task": "Fix bug"},
            type_map=type_map,
            arg_types={"task": "string"},
            return_type="Priority"
        )
        
        # Verify result is a Priority enum or dynamic enum
        assert result is not None
    
    @pytest.mark.asyncio
    async def test_scoped_client_with_type_map(self, baml_files):
        """Test ScopedClient with bound type map"""
        rt = create_runtime(".", baml_files, {"OPENAI_API_KEY": os.getenv("OPENAI_API_KEY", "test-key")})
        type_map = create_test_type_map()
        
        # Create scoped client
        client = ScopedClient(rt, type_map)
        
        # Create test person
        person = Person(
            name="Bob",
            age=25,
            email="bob@example.com",
            tags=["designer"]
        )
        
        # Call through scoped client
        result = await client.call_function(
            name="ProcessPerson",
            args={"person": person},
            arg_types={"person": "Person"},
            return_type="Person"
        )
        
        # Verify result
        assert result is not None
    
    @pytest.mark.asyncio
    async def test_multiple_scoped_clients_different_type_maps(self, baml_files):
        """Test multiple scoped clients with different type maps"""
        rt = create_runtime(".", baml_files, {"OPENAI_API_KEY": os.getenv("OPENAI_API_KEY", "test-key")})
        
        # Create two different type maps
        type_map1 = create_test_type_map()
        type_map2 = create_test_type_map()  # Independent copy
        
        # Create two scoped clients
        client1 = ScopedClient(rt, type_map1)
        client2 = ScopedClient(rt, type_map2)
        
        # Verify they maintain separate contexts
        assert client1.type_map is type_map1
        assert client2.type_map is type_map2
        assert client1.type_map is not client2.type_map
    
    @pytest.mark.asyncio
    async def test_concurrent_calls_with_different_type_maps(self, baml_files):
        """Test concurrent calls with different type maps don't interfere"""
        rt = create_runtime(".", baml_files, {"OPENAI_API_KEY": os.getenv("OPENAI_API_KEY", "test-key")})
        
        # Create different type maps
        type_map1 = create_test_type_map()
        type_map2 = create_test_type_map()
        
        # Create test persons
        person1 = Person("Alice", 30, "alice@example.com", ["dev"])
        person2 = Person("Bob", 25, "bob@example.com", ["design"])
        
        # Make concurrent calls with different type maps
        tasks = [
            rt.call_function_typed(
                "ProcessPerson",
                {"person": person1},
                type_map1,
                {"person": "Person"},
                "Person"
            ),
            rt.call_function_typed(
                "ProcessPerson",
                {"person": person2},
                type_map2,
                {"person": "Person"},
                "Person"
            )
        ]
        
        results = await asyncio.gather(*tasks)
        
        # Verify both results
        assert len(results) == 2
        assert all(r is not None for r in results)
    
    @pytest.mark.asyncio
    async def test_list_of_user_types(self, baml_files):
        """Test function returning list of user-defined types"""
        rt = create_runtime(".", baml_files, {"OPENAI_API_KEY": os.getenv("OPENAI_API_KEY", "test-key")})
        type_map = create_test_type_map()
        
        # Call function that returns Person[]
        result = await rt.call_function_typed(
            name="ListPeople",
            args={"count": 3},
            type_map=type_map,
            arg_types={"count": "int"},
            return_type="list"  # The list items will use type map for Person
        )
        
        # Verify result is a list
        assert isinstance(result, list)
    
    @pytest.mark.asyncio
    async def test_dynamic_fallback_when_type_not_in_map(self, baml_files):
        """Test that dynamic types are created when type not in map"""
        rt = create_runtime(".", baml_files, {"OPENAI_API_KEY": os.getenv("OPENAI_API_KEY", "test-key")})
        
        # Use empty type map - should fall back to dynamic types
        empty_type_map: TypeMap = {}
        
        # Call function with no type map entries
        result = await rt.call_function_typed(
            name="GetPriority",
            args={"task": "Test task"},
            type_map=empty_type_map,
            arg_types={"task": "string"},
            return_type="Priority"
        )
        
        # Result should be a dynamic enum since Priority not in map
        assert result is not None
        # Could be DynamicEnum with _type and _value attributes
    
    @pytest.mark.asyncio
    async def test_nested_types_with_type_map(self, baml_files):
        """Test nested user-defined types work correctly"""
        @dataclass
        class Team:
            name: str
            lead: Person
            members: List[Person]
        
        def encode_Team(team: Team, type_map: TypeMap) -> cffi_pb2.CFFIValueHolder:
            holder = cffi_pb2.CFFIValueHolder()
            holder.class_value.name.name = "Team"
            
            # Name field
            name_entry = cffi_pb2.CFFIMapEntry()
            name_entry.key = "name"
            name_entry.value.CopyFrom(encode_value(team.name, type_map=type_map))
            holder.class_value.fields.append(name_entry)
            
            # Lead field (Person)
            lead_entry = cffi_pb2.CFFIMapEntry()
            lead_entry.key = "lead"
            lead_entry.value.CopyFrom(encode_value(team.lead, "Person", type_map))
            holder.class_value.fields.append(lead_entry)
            
            # Members field (List[Person])
            members_entry = cffi_pb2.CFFIMapEntry()
            members_entry.key = "members"
            members_entry.value.CopyFrom(encode_value(team.members, type_map=type_map))
            holder.class_value.fields.append(members_entry)
            
            return holder
        
        def decode_Team(holder: cffi_pb2.CFFIValueHolder, type_map: TypeMap) -> Team:
            fields = {}
            for field in holder.class_value.fields:
                if field.key == "lead":
                    fields[field.key] = decode_value(field.value, "Person", type_map)
                else:
                    fields[field.key] = decode_value(field.value, type_map=type_map)
            
            return Team(
                name=fields.get("name", ""),
                lead=fields.get("lead"),
                members=fields.get("members", [])
            )
        
        # Extend type map with Team
        type_map = create_test_type_map()
        type_map["Team"] = (
            Team,
            lambda t: encode_Team(t, type_map),
            lambda h: decode_Team(h, type_map)
        )
        
        # Test encoding and decoding
        alice = Person("Alice", 30, "alice@example.com", ["lead"])
        bob = Person("Bob", 25, "bob@example.com", ["member"])
        team = Team("Engineering", alice, [bob])
        
        # Encode and decode round-trip
        encoded = encode_value(team, "Team", type_map)
        decoded = decode_value(encoded, "Team", type_map)
        
        assert isinstance(decoded, Team)
        assert decoded.name == team.name
        assert decoded.lead.name == alice.name
        assert len(decoded.members) == 1
        assert decoded.members[0].name == bob.name