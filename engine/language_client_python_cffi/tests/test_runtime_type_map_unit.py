"""
Unit tests for runtime type map integration - no API calls
"""
import pytest
from typing import List, Dict
from dataclasses import dataclass

from baml_py_cffi.serde.type_map import TypeMap
from baml_py_cffi.serde.encode import encode_value
from baml_py_cffi.serde.decode import decode_value
from baml_py_cffi.serde import cffi_pb2
from baml_py_cffi.client import ScopedClient


@dataclass
class Book:
    """Test data class"""
    title: str
    author: str
    year: int
    
    def __eq__(self, other):
        return (
            isinstance(other, Book) and
            self.title == other.title and
            self.author == other.author and
            self.year == other.year
        )


def encode_Book(book: Book, type_map: TypeMap) -> cffi_pb2.CFFIValueHolder:
    """Encoder for Book class"""
    holder = cffi_pb2.CFFIValueHolder()
    holder.class_value.name.name = "Book"
    
    # Title field
    title_entry = cffi_pb2.CFFIMapEntry()
    title_entry.key = "title"
    title_entry.value.CopyFrom(encode_value(book.title, type_map=type_map))
    holder.class_value.fields.append(title_entry)
    
    # Author field
    author_entry = cffi_pb2.CFFIMapEntry()
    author_entry.key = "author"
    author_entry.value.CopyFrom(encode_value(book.author, type_map=type_map))
    holder.class_value.fields.append(author_entry)
    
    # Year field
    year_entry = cffi_pb2.CFFIMapEntry()
    year_entry.key = "year"
    year_entry.value.CopyFrom(encode_value(book.year, type_map=type_map))
    holder.class_value.fields.append(year_entry)
    
    return holder


def decode_Book(holder: cffi_pb2.CFFIValueHolder, type_map: TypeMap) -> Book:
    """Decoder for Book class"""
    fields = {}
    for field in holder.class_value.fields:
        fields[field.key] = decode_value(field.value, type_map=type_map)
    
    return Book(
        title=fields.get("title", ""),
        author=fields.get("author", ""),
        year=fields.get("year", 0)
    )


class TestRuntimeTypeMapUnit:
    """Unit tests for type map functionality"""
    
    def test_type_map_creation(self):
        """Test creating a type map with custom types"""
        type_map: TypeMap = {}
        
        # Register Book type
        type_map["Book"] = (
            Book,
            lambda b: encode_Book(b, type_map),
            lambda h: decode_Book(h, type_map)
        )
        
        assert "Book" in type_map
        assert len(type_map) == 1
    
    def test_encode_decode_custom_type(self):
        """Test encoding and decoding a custom type"""
        type_map: TypeMap = {}
        type_map["Book"] = (
            Book,
            lambda b: encode_Book(b, type_map),
            lambda h: decode_Book(h, type_map)
        )
        
        # Create test book
        book = Book(
            title="The Python Guide",
            author="John Doe",
            year=2024
        )
        
        # Encode
        encoded = encode_value(book, "Book", type_map)
        
        # Verify it's a CFFIValueHolder with class_value
        assert encoded.HasField("class_value")
        assert encoded.class_value.name.name == "Book"
        
        # Decode
        decoded = decode_value(encoded, "Book", type_map)
        
        # Verify round-trip
        assert decoded == book
    
    def test_encode_without_type_map(self):
        """Test encoding falls back to checking class name"""
        type_map: TypeMap = {}
        type_map["Book"] = (
            Book,
            lambda b: encode_Book(b, type_map),
            lambda h: decode_Book(h, type_map)
        )
        
        book = Book("Test", "Author", 2024)
        
        # Should find by class name even without type_name arg
        encoded = encode_value(book, type_map=type_map)
        assert encoded.HasField("class_value")
    
    def test_decode_with_dynamic_fallback(self):
        """Test decoding creates dynamic class when type not in map"""
        # Create encoded book
        holder = cffi_pb2.CFFIValueHolder()
        holder.class_value.name.name = "UnknownType"
        
        field = cffi_pb2.CFFIMapEntry()
        field.key = "data"
        field.value.string_value = "test"
        holder.class_value.fields.append(field)
        
        # Decode without type in map
        empty_map: TypeMap = {}
        result = decode_value(holder, type_map=empty_map)
        
        # Should get a dynamic class
        assert hasattr(result, "_name")
        assert hasattr(result, "_fields")
        assert result._name == "UnknownType"
        assert result._fields["data"] == "test"
    
    def test_multiple_type_maps_independence(self):
        """Test that multiple type maps are independent"""
        type_map1: TypeMap = {}
        type_map2: TypeMap = {}
        
        # Register same type differently in each map
        type_map1["Book"] = (
            Book,
            lambda b: encode_Book(b, type_map1),
            lambda h: decode_Book(h, type_map1)
        )
        
        type_map2["Book"] = (
            Book,
            lambda b: encode_Book(b, type_map2),
            lambda h: decode_Book(h, type_map2)
        )
        
        # Maps should be independent
        assert type_map1 is not type_map2
        assert type_map1["Book"] != type_map2["Book"]  # Different lambda instances
    
    def test_scoped_client_properties(self):
        """Test ScopedClient holds references correctly"""
        # Mock runtime (we don't need actual runtime for this test)
        class MockRuntime:
            async def call_function_typed(self, *args, **kwargs):
                return None
        
        runtime = MockRuntime()
        type_map: TypeMap = {}
        
        # Create scoped client
        client = ScopedClient(runtime, type_map)
        
        # Verify properties
        assert client.runtime is runtime
        assert client.type_map is type_map
    
    def test_nested_types_encoding(self):
        """Test encoding nested custom types"""
        @dataclass
        class Library:
            name: str
            books: List[Book]
        
        def encode_Library(lib: Library, type_map: TypeMap) -> cffi_pb2.CFFIValueHolder:
            holder = cffi_pb2.CFFIValueHolder()
            holder.class_value.name.name = "Library"
            
            # Name field
            name_entry = cffi_pb2.CFFIMapEntry()
            name_entry.key = "name"
            name_entry.value.CopyFrom(encode_value(lib.name, type_map=type_map))
            holder.class_value.fields.append(name_entry)
            
            # Books field - encode as list with type map
            books_list = cffi_pb2.CFFIValueList()
            for book in lib.books:
                books_list.values.append(encode_value(book, "Book", type_map))
            
            books_entry = cffi_pb2.CFFIMapEntry()
            books_entry.key = "books"
            books_entry.value.list_value.CopyFrom(books_list)
            holder.class_value.fields.append(books_entry)
            
            return holder
        
        def decode_Library(holder: cffi_pb2.CFFIValueHolder, type_map: TypeMap) -> Library:
            fields = {}
            for field in holder.class_value.fields:
                if field.key == "books":
                    # Decode list of books
                    books = []
                    for book_holder in field.value.list_value.values:
                        books.append(decode_value(book_holder, "Book", type_map))
                    fields[field.key] = books
                else:
                    fields[field.key] = decode_value(field.value, type_map=type_map)
            
            return Library(
                name=fields.get("name", ""),
                books=fields.get("books", [])
            )
        
        # Create type map with both types
        type_map: TypeMap = {}
        type_map["Book"] = (
            Book,
            lambda b: encode_Book(b, type_map),
            lambda h: decode_Book(h, type_map)
        )
        type_map["Library"] = (
            Library,
            lambda l: encode_Library(l, type_map),
            lambda h: decode_Library(h, type_map)
        )
        
        # Create test data
        library = Library(
            name="City Library",
            books=[
                Book("Book 1", "Author 1", 2023),
                Book("Book 2", "Author 2", 2024)
            ]
        )
        
        # Encode and decode
        encoded = encode_value(library, "Library", type_map)
        decoded = decode_value(encoded, "Library", type_map)
        
        # Verify
        assert decoded.name == library.name
        assert len(decoded.books) == 2
        assert decoded.books[0].title == "Book 1"
        assert decoded.books[1].author == "Author 2"