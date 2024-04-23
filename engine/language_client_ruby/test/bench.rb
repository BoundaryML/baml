require "benchmark"
require_relative "../lib/rust_blank"

n = 1_000_000
Benchmark.bmbm do |x|
  x.report("empty") do
    n.times {"".blank?}
  end

  x.report("blank") do
    n.times {" ".blank?}
  end

  x.report("present") do
    n.times {"x".blank?}
  end

  x.report("lots of spaces blank") do
    n.times {"                                          ".blank?}
  end

  x.report("lots of spaces present") do
    n.times {"                                          x".blank?}
  end

  x.report("blank US-ASCII") do
    s = " ".encode("US-ASCII")
    n.times {s.blank?}
  end

  x.report("blank non-utf-8") do
    s = " ".encode("UTF-16LE")
    n.times {s.blank?}
  end
end
