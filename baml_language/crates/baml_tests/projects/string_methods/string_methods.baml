// Test: string method calls should produce no diagnostics

template_string Sep(n: int) #"
  {{- "{:,}".format(n) -}}
"#

template_string Upper(s: string) #"
  {{ s.upper() }}
"#

template_string StartsWith(s: string) #"
  {% if s.startswith("hello") %}yes{% endif %}
"#

template_string Split(s: string) #"
  {% for part in s.split(",") %}{{ part }}{% endfor %}
"#
