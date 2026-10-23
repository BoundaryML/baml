# frozen_string_literal: true

require "base64"

module Baml
  # Portable media content. Construction copies its strings so neither the
  # caller nor an engine handle can mutate a value after it has been built.
  class Media
    attr_reader :url, :base64, :mime_type, :name

    def self.from_url(url, mime_type: nil)
      new(url: url, mime_type: mime_type)
    end

    def self.from_base64(base64, mime_type: nil)
      new(base64: base64, mime_type: mime_type)
    end

    def self.from_file(file, mime_type: nil)
      from_file_content(file, Base64.strict_encode64(File.binread(file)), mime_type: mime_type)
    end

    def self.from_file_content(file, base64, mime_type: nil)
      name = File.basename(file)
      new(base64: base64, name: name, mime_type: mime_type)
    end

    def initialize(url: nil, base64: "", mime_type: nil, name: nil)
      @url, @base64, @mime_type, @name = [url, base64, mime_type, name].map do |value|
        raise TypeError, "Media content must be a String" unless value.nil? || value.is_a?(String)

        value&.dup&.freeze
      end
      freeze
    end
    private_class_method :new
  end

  class Image < Media; end
  class Audio < Media; end
  class Video < Media; end
  class Pdf < Media; end
end
