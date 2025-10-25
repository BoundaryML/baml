"use client";

import { useState } from "react";
import { useExtractResume } from "./baml_client/react/hooks";
import { Resume } from "./baml_client/types";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

export default function Home() {
  const [result, setResult] = useState<Resume | null>(null);
  const [input, setInput] = useState(`John Doe
Software Engineer
john.doe@email.com | (555) 123-4567 | LinkedIn: linkedin.com/in/johndoe

EXPERIENCE

Senior Software Engineer | Tech Corp
January 2020 - Present
• Led development of microservices architecture serving 1M+ users daily
• Mentored team of 5 junior developers and conducted code reviews
• Reduced API response time by 40% through optimization

Full Stack Developer | StartupXYZ
June 2018 - December 2019
• Built React/Node.js web application from scratch
• Implemented CI/CD pipeline using GitHub Actions
• Collaborated with design team to improve UX

Junior Developer | Digital Agency
May 2016 - May 2018
• Developed responsive websites using HTML, CSS, JavaScript
• Maintained WordPress sites for 20+ clients

SKILLS
JavaScript, TypeScript, Python, React, Node.js, Express, PostgreSQL, MongoDB, AWS, Docker, Git, Agile, REST APIs, GraphQL

EDUCATION
Bachelor of Science in Computer Science
University of Technology | 2016`);

  const { isLoading, error, mutate } = useExtractResume({
    onFinalData(response) {
      if (response) {
        setResult(response);
      }
    },
    onStreamData(response) {
      if (response) {
        setResult({
          name: response.name ?? "",
          email: response.email ?? "",
          experience: response.experience,
          skills: response.skills,
        });
      }
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (input.trim()) {
      mutate(input);
    }
  };

  return (
    <div className="min-h-screen bg-gray-50 py-12 px-4 sm:px-6 lg:px-8">
      <div className="max-w-4xl mx-auto">
        <Card className="mb-8">
          <CardHeader>
            <CardTitle className="text-2xl">Resume Extractor</CardTitle>
            <CardDescription>
              Paste resume text below to extract structured information
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit} className="space-y-4">
              <Textarea
                placeholder="Paste resume text here..."
                value={input}
                onChange={(e) => setInput(e.target.value)}
                rows={8}
                className="w-full"
              />
              <Button
                type="submit"
                disabled={isLoading || !input.trim()}
                className="w-full sm:w-auto"
              >
                {isLoading ? "Extracting..." : "Extract Resume"}
              </Button>
            </form>
          </CardContent>
        </Card>

        {error && (
          <Card className="mb-8 border-red-200">
            <CardContent className="pt-6">
              <p className="text-red-600">Error: {error.message}</p>
            </CardContent>
          </Card>
        )}

        {result && (
          <Card>
            <CardHeader>
              <CardTitle>Extracted Information</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <h3 className="font-semibold text-sm text-gray-500 mb-1">Name</h3>
                <p className="text-lg">{result.name || "Not found"}</p>
              </div>

              <div>
                <h3 className="font-semibold text-sm text-gray-500 mb-1">Email</h3>
                <p className="text-lg">{result.email || "Not found"}</p>
              </div>

              {result.experience && result.experience.length > 0 && (
                <div>
                  <h3 className="font-semibold text-sm text-gray-500 mb-2">Experience</h3>
                  <ul className="space-y-1">
                    {result.experience.map((exp, index) => (
                      <li key={index} className="text-gray-700">
                        • {exp}
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {result.skills && result.skills.length > 0 && (
                <div>
                  <h3 className="font-semibold text-sm text-gray-500 mb-2">Skills</h3>
                  <div className="flex flex-wrap gap-2">
                    {result.skills.map((skill, index) => (
                      <span
                        key={index}
                        className="bg-gray-100 px-3 py-1 rounded-full text-sm text-gray-700"
                      >
                        {skill}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}