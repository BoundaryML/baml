# frozen_string_literal: true

require_relative "../lib/baml"

require 'minitest/autorun'
require 'minitest/reporters'
require "sorbet-coerce"
require "sorbet-runtime"
require "sorbet-struct-comparable"

# Test that ruby_to_json.rs correctly ingests Ruby objects and returns JSON

class BookGenre < T::Enum
  enums do
    FICTION = new('FICTION')
    NON_FICTION = new('NON_FICTION')
    MYSTERY = new('MYSTERY')
    ROMANCE = new('ROMANCE')
    SCI_FI = new('SCI_FI')
  end
end

class DigitalEdition < T::Struct
  extend T::Sig
  if defined?(T::Struct::ActsAsComparable)
    include T::Struct::ActsAsComparable
  end

  const :url, String
end

class PrintEdition < T::Struct
  extend T::Sig
  if defined?(T::Struct::ActsAsComparable)
    include T::Struct::ActsAsComparable
  end

  const :edition, String
  const :publication_year, Integer
end

class Book < T::Struct
  extend T::Sig
  if defined?(T::Struct::ActsAsComparable)
    include T::Struct::ActsAsComparable
  end

  const :isbn, String
  const :name, String
  const :page_count, Integer
  const :price, Float
  const :genre, BookGenre
  const :authors, T::Array[String]
  const :publisher, T.nilable(String)
  const :format, T.any(String, T::Array[String])
  const :edition, T.any(DigitalEdition, PrintEdition)
end

BOOK_OBJ = Book.new(
  isbn: "978-3-16-148410-0",
  name: "The Great Gatsby",
  page_count: 180,
  price: 12.99,
  genre: BookGenre::FICTION,
  authors: ["F. Scott Fitzgerald"],
  publisher: "Scribner",
  format: ["Hardcover", "Paperback"],
  edition: PrintEdition.new(edition: "First", publication_year: 1925)
)
BOOK_HASH = {
  "isbn"=>"978-3-16-148410-0",
  "name"=>"The Great Gatsby",
  "page_count"=>180,
  "price"=>12.99,
  "genre"=>"FICTION",
  "authors"=>["F. Scott Fitzgerald"],
  "publisher"=>"Scribner",
  "format"=>["Hardcover", "Paperback"],
  "edition"=>{"edition"=>"First", "publication_year"=>1925},
}

describe "converting Ruby objects to JSON" do
  it "BamlConvert handles JSON to Ruby" do
    converted = Baml::convert_to(Book).from(BOOK_HASH)
    assert_equal(converted, BOOK_OBJ)

    converted = Baml::convert_to(BookGenre).from("NON_FICTION")
    assert_equal(converted, BookGenre::NON_FICTION)
  end

  it "T::Struct.serialize does not handle unions correctly" do
    refute_equal(
      BOOK_OBJ.serialize,
      BOOK_HASH
    )
    assert_equal(
      BOOK_OBJ.serialize,
      BOOK_HASH.merge(
        "edition" => BOOK_OBJ.edition
      )
    )
  end

  it "ruby_to_json.rs converts Ruby types correctly" do
    assert_equal(
      Baml::Ffi::roundtrip(BOOK_OBJ),
      BOOK_HASH
    )
    assert_equal(
      Baml::Ffi::roundtrip("Hello, world!"),
      "Hello, world!"
    )
    assert_equal(
      Baml::Ffi::roundtrip(["Hello, world!"]),
      ["Hello, world!"]
    )
    assert_equal(
      Baml::Ffi::roundtrip(BookGenre::FICTION),
      "FICTION"
    )
    assert_equal(
      Baml::Ffi::roundtrip([BookGenre::FICTION]),
      ["FICTION"]
    )
    assert_equal(
      Baml::Ffi::roundtrip([BookGenre::FICTION, [BookGenre::NON_FICTION]]),
      ["FICTION", ["NON_FICTION"]]
    )
  end
end

describe "converting JSON to Ruby objects" do
  it "handles complex objects" do
    skip "unions of types are not handled correctly"

    assert_equal(
      TypeCoerce[Book].new.from(BOOK_HASH),
      BOOK_OBJ
    )
  end

  it "parses enums" do
    assert_equal(
      TypeCoerce[BookGenre].new.from("FICTION"),
      BookGenre::FICTION
    )
  end

  class Person < T::Struct
    extend T::Sig
    if defined?(T::Struct::ActsAsComparable)
      include T::Struct::ActsAsComparable
    end

    const :address, T.nilable(String)
  end

  # This is the current behavior of JSON-ish with respect to nulls
  it "parses unset fields as nil" do
    assert_equal(
      TypeCoerce[Person].new.from({}),
      Person.new(address: nil)
    )
    assert_equal(
      TypeCoerce[Person].new.from({:address => nil}),
      Person.new(address: nil)
    )
    assert_equal(
      TypeCoerce[Person].new.from({"address" => nil}),
      Person.new(address: nil)
    )
  end
end

Minitest::Reporters.use! Minitest::Reporters::SpecReporter.new
