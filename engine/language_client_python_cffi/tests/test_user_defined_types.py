import pytest
from .codegen_example import (
    User, Status, UserProfile, 
    create_type_map
)
from baml_py_cffi.serde.encode import encode_value
from baml_py_cffi.serde.decode import decode_value, decode_dynamic_class, decode_dynamic_enum
from baml_py_cffi.serde import cffi_pb2


class TestUserDefinedTypes:
    """Test user-defined types with type map."""
    
    def setup_method(self):
        """Set up type map for tests."""
        self.type_map = create_type_map()
    
    def test_user_class_round_trip(self):
        """Test User class encoding and decoding."""
        # Create a User instance
        user = User(
            name="Alice",
            age=30,
            email="alice@example.com",
            tags=["python", "developer", "remote"]
        )
        
        # Encode with type map
        holder = encode_value(user, type_name="User", type_map=self.type_map)
        
        # Verify it's encoded as a class
        assert holder.HasField('class_value')
        assert holder.class_value.name.name == "User"
        
        # Decode back
        decoded = decode_value(holder, type_name="User", type_map=self.type_map)
        
        # Verify round-trip
        assert isinstance(decoded, User)
        assert decoded.name == "Alice"
        assert decoded.age == 30
        assert decoded.email == "alice@example.com"
        assert decoded.tags == ["python", "developer", "remote"]
    
    def test_status_enum_round_trip(self):
        """Test Status enum encoding and decoding."""
        # Test each enum value
        for status in [Status.ACTIVE, Status.INACTIVE, Status.PENDING]:
            # Encode with type map
            holder = encode_value(status, type_name="Status", type_map=self.type_map)
            
            # Verify it's encoded as an enum
            assert holder.HasField('enum_value')
            assert holder.enum_value.name.name == "Status"
            assert holder.enum_value.value == status.value
            
            # Decode back
            decoded = decode_value(holder, type_name="Status", type_map=self.type_map)
            
            # Verify round-trip
            assert isinstance(decoded, Status)
            assert decoded == status
    
    def test_user_profile_nested_types(self):
        """Test UserProfile with nested user-defined types."""
        # Create a UserProfile with nested types
        user = User(
            name="Bob",
            age=25,
            email="bob@example.com",
            tags=["junior", "frontend"]
        )
        
        profile = UserProfile(
            user=user,
            status=Status.ACTIVE,
            metadata={"level": 3, "score": 95.5, "verified": True}
        )
        
        # Encode with type map
        holder = encode_value(profile, type_name="UserProfile", type_map=self.type_map)
        
        # Verify structure
        assert holder.HasField('class_value')
        assert holder.class_value.name.name == "UserProfile"
        
        # Decode back
        decoded = decode_value(holder, type_name="UserProfile", type_map=self.type_map)
        
        # Verify round-trip
        assert isinstance(decoded, UserProfile)
        assert isinstance(decoded.user, User)
        assert decoded.user.name == "Bob"
        assert decoded.user.age == 25
        assert isinstance(decoded.status, Status)
        assert decoded.status == Status.ACTIVE
        assert decoded.metadata == {"level": 3, "score": 95.5, "verified": True}
    
    def test_list_of_users(self):
        """Test list of User objects."""
        users = [
            User("Alice", 30, "alice@example.com", ["python"]),
            User("Bob", 25, "bob@example.com", ["javascript"]),
            User("Charlie", 35, "charlie@example.com", ["go", "rust"])
        ]
        
        # Encode list with type map
        holder = encode_value(users, type_map=self.type_map)
        
        # Verify it's a list
        assert holder.HasField('list_value')
        assert len(holder.list_value.values) == 3
        
        # Decode back
        decoded = decode_value(holder, type_map=self.type_map)
        
        # Verify round-trip
        assert isinstance(decoded, list)
        assert len(decoded) == 3
        for i, user in enumerate(decoded):
            assert isinstance(user, User)
            assert user.name == users[i].name
            assert user.age == users[i].age
    
    def test_map_with_user_values(self):
        """Test map with User objects as values."""
        user_map = {
            "alice": User("Alice", 30, "alice@example.com", ["python"]),
            "bob": User("Bob", 25, "bob@example.com", ["javascript"])
        }
        
        # Encode map with type map
        holder = encode_value(user_map, type_map=self.type_map)
        
        # Verify it's a map
        assert holder.HasField('map_value')
        assert len(holder.map_value.entries) == 2
        
        # Decode back
        decoded = decode_value(holder, type_map=self.type_map)
        
        # Verify round-trip
        assert isinstance(decoded, dict)
        assert len(decoded) == 2
        assert isinstance(decoded["alice"], User)
        assert decoded["alice"].name == "Alice"
        assert isinstance(decoded["bob"], User)
        assert decoded["bob"].name == "Bob"
    
    def test_dynamic_class_fallback(self):
        """Test dynamic class creation when type not in map."""
        # Create a class value holder manually
        holder = cffi_pb2.CFFIValueHolder()
        holder.class_value.CopyFrom(cffi_pb2.CFFIValueClass())
        holder.class_value.name.CopyFrom(cffi_pb2.CFFITypeName())
        holder.class_value.name.name = "UnknownClass"
        holder.class_value.name.namespace = cffi_pb2.CFFITypeNamespace.TYPES
        
        # Add some fields
        field1 = cffi_pb2.CFFIMapEntry()
        field1.key = "field1"
        field1.value.string_value = "value1"
        holder.class_value.fields.append(field1)
        
        field2 = cffi_pb2.CFFIMapEntry()
        field2.key = "field2"
        field2.value.int_value = 42
        holder.class_value.fields.append(field2)
        
        # Decode without type map entry
        decoded = decode_value(holder, type_map=self.type_map)
        
        # Should get a dynamic class
        assert decoded._name == "UnknownClass"
        assert decoded._fields["field1"] == "value1"
        assert decoded._fields["field2"] == 42
        
        # Test attribute access
        assert decoded.field1 == "value1"
        assert decoded.field2 == 42
    
    def test_dynamic_enum_fallback(self):
        """Test dynamic enum creation when type not in map."""
        # Create an enum value holder manually
        holder = cffi_pb2.CFFIValueHolder()
        holder.enum_value.CopyFrom(cffi_pb2.CFFIValueEnum())
        holder.enum_value.name.CopyFrom(cffi_pb2.CFFITypeName())
        holder.enum_value.name.name = "UnknownEnum"
        holder.enum_value.name.namespace = cffi_pb2.CFFITypeNamespace.TYPES
        holder.enum_value.value = "unknown_value"
        holder.enum_value.is_dynamic = True
        
        # Decode without type map entry
        decoded = decode_value(holder, type_map=self.type_map)
        
        # Should get a dynamic enum
        assert decoded._type == "UnknownEnum"
        assert decoded._value == "unknown_value"
        assert str(decoded) == "unknown_value"
    
    def test_type_map_lookup_by_class_name(self):
        """Test that encoder finds types by class name."""
        # Create a User without specifying type_name
        user = User(
            name="Test",
            age=20,
            email="test@example.com",
            tags=[]
        )
        
        # Encode without type_name, should find by class name
        holder = encode_value(user, type_map=self.type_map)
        
        # Verify it worked
        assert holder.HasField('class_value')
        assert holder.class_value.name.name == "User"
        
        # Decode back
        decoded = decode_value(holder, type_map=self.type_map)
        assert isinstance(decoded, User)
        assert decoded.name == "Test"
    
    def test_multiple_type_maps_independence(self):
        """Test that multiple type maps can coexist independently."""
        # Create two independent type maps
        type_map1 = create_type_map()
        type_map2 = create_type_map()
        
        # Create users with different type maps
        user1 = User("User1", 30, "user1@example.com", ["map1"])
        user2 = User("User2", 40, "user2@example.com", ["map2"])
        
        # Encode with different type maps
        holder1 = encode_value(user1, type_name="User", type_map=type_map1)
        holder2 = encode_value(user2, type_name="User", type_map=type_map2)
        
        # Decode with corresponding type maps
        decoded1 = decode_value(holder1, type_name="User", type_map=type_map1)
        decoded2 = decode_value(holder2, type_name="User", type_map=type_map2)
        
        # Verify independence
        assert decoded1.name == "User1"
        assert decoded2.name == "User2"
        assert decoded1.tags == ["map1"]
        assert decoded2.tags == ["map2"]