# frozen_string_literal: true

require "test/unit"
require_relative "../lib/rust_blank"

class BlankTest < Test::Unit::TestCase

  def test_blank?
    assert { "".blank? }
    assert { "   ".blank? }
    assert { "  \n\t  \r ".blank? }
    assert { "　".blank? }
    assert { "\u00a0".blank? }
    assert { " ".encode("UTF-16LE").blank? }
  end

  def test_not_blank?
    assert { !"a".blank? }
    assert { !"my value".encode("UTF-16LE").blank? }
  end

end
