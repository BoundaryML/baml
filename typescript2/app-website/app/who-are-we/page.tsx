import { Github, MessageCircle, Star, Users, Zap } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { FooterSection } from '@/components/footer-section';
import { MagicCard } from '@/components/magicui/magic-card';
import { Navbar } from '@/components/navbar';
import { Button } from '@/components/ui/button';

const teamMembers = [
  {
    description:
      'Former Google and Microsoft computer vision engineer. Built BAML to make AI development as reliable as traditional software.',
    image: '/profile-vbv.jpeg',
    linkedin: 'https://www.linkedin.com/in/vaigup',
    name: 'Vaibhav Gupta',
    role: 'CEO & Co-founder',
  },
  {
    description:
      'Former Amazon engineer who built AI systems at scale. Believes the future of AI development is type-safe and developer-friendly.',
    image: '/aaronv.jpg',
    linkedin: 'https://www.linkedin.com/in/aaron-villalpando-99284576/',
    name: 'Aaron Villalpando',
    role: 'CTO & Co-founder',
  },
  {
    description:
      'Full-stack engineer with expertise in developer tools and AI systems. Focused on making complex AI development accessible to everyone.',
    image: '/profile-sam.png',
    linkedin: 'https://www.linkedin.com/in/sxlijin/',
    name: 'Sam Lijin',
    role: 'Engineer',
  },
  {
    description:
      'Full-stack engineer passionate about developer experience and building robust AI systems. Works on core BAML infrastructure.',
    image: '/antonio.jpeg',
    linkedin: 'https://www.linkedin.com/in/antoniosarosi/',
    name: 'Antonio Sarosi',
    role: 'Engineer',
  },
  {
    description:
      'Product-focused engineer who bridges the gap between user needs and technical implementation. Makes AI development intuitive.',
    image: '/greg.jpg',
    linkedin: 'https://www.linkedin.com/in/greg-hale-5684b1bb/',
    name: 'Greg Hale',
    role: 'Engineer',
  },
  {
    description:
      'Full-stack engineer specializing in React/Next.js integration and developer tooling. Makes BAML work seamlessly in modern web apps.',
    image: '/seawatts.png',
    linkedin: 'https://www.linkedin.com/in/seawatts',
    name: 'Chris Watts',
    role: 'Engineer',
  },
  {
    description:
      'Full-stack engineer specializing in React/Next.js integration and developer tooling. Makes BAML work seamlessly in modern web apps.',
    image: '/profile-anish.png',
    linkedin: 'https://www.linkedin.com/in/anish-palakurthi/',
    name: 'Anish',
    role: 'Intern S24',
  },
  {
    description:
      'Engineer focused on developer experience and VS Code integration. Ensures BAML works perfectly in your favorite editor.',
    image: '/rahult.jpg',
    linkedin: 'https://www.linkedin.com/in/ba11b0y/',
    name: 'Rahul',
    role: 'Intern S25',
  },
  {
    description:
      'Core systems engineer building the BAML language and tooling ecosystem. Makes the magic happen behind the scenes.',
    image: '/egor.jpg',
    linkedin: 'https://www.linkedin.com/in/egor-l/',
    name: 'Egor',
    role: 'Intern S25',
  },
];

const companyStats = [
  { icon: Star, label: 'GitHub Stars', value: '5,000+' },
  { icon: Users, label: 'Active Developers', value: '1,000+' },
  { icon: MessageCircle, label: 'Discord Members', value: '700+' },
  { icon: Zap, label: 'Core Engineers', value: '7' },
];

const companyValues = [
  {
    description:
      'We believe AI development should be as reliable as traditional software development.',
    title: 'Type Safety First',
  },
  {
    description:
      'Every tool we build is designed to make developers more productive and happy.',
    title: 'Developer Experience',
  },
  {
    description:
      'BAML is open source because we believe the best tools are built in the open.',
    title: 'Open Source',
  },
  {
    description:
      'We listen to our community and build what developers actually need.',
    title: 'Community Driven',
  },
];

export default function WhoAreWePage() {
  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center justify-center min-h-screen w-full">
        {/* Hero Section */}
        <section className="relative flex w-full items-center justify-center px-4 pt-20 md:pt-32">
          <div className="flex flex-col items-center justify-center space-y-8 text-center max-w-4xl">
            <h1 className="bg-gradient-to-br dark:from-white from-black from-30% dark:to-white/40 to-black/40 bg-clip-text py-4 sm:py-6 text-5xl font-medium leading-none tracking-tighter text-transparent text-balance sm:text-5xl md:text-6xl lg:text-6xl xl:text-7xl translate-y-[-1rem] animate-fade-in opacity-0 [--animation-delay:200ms]">
              Who are we?
            </h1>
            <div className="space-y-6">
              <p className="text-xl md:text-2xl text-muted-foreground">
                We believe AI needs its own language—the first designed with
                LLMs in mind. So we're building it:{' '}
                <span className="text-foreground font-semibold">
                  cognitive coding for the future of software
                </span>
                .
              </p>
              <p className="text-xl md:text-2xl text-muted-foreground">
                Yes, we're that crazy.
              </p>
              <p className="text-lg text-muted-foreground">
                (We literally use Notion to present slides)
              </p>
            </div>
          </div>
        </section>

        {/* Company Stats */}
        {/* <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="text-center mb-12">
              <h2 className="text-3xl font-bold mb-4">Growing Fast</h2>
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                From 2 engineers to a thriving community of developers building
                the future of AI.
              </p>
            </div>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-8">
              {companyStats.map((stat) => (
                <div className="text-center" key={stat.label}>
                  <div className="flex justify-center mb-4">
                    <stat.icon className="h-8 w-8 text-primary" />
                  </div>
                  <div className="text-3xl font-bold mb-2">{stat.value}</div>
                  <div className="text-muted-foreground">{stat.label}</div>
                </div>
              ))}
            </div>
          </div>
        </section> */}

        {/* Company Values */}
        {/* <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="text-center mb-12">
              <h2 className="text-3xl font-bold mb-4">Our Values</h2>
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                The principles that guide everything we build at Boundary.
              </p>
            </div>
            <div className="grid md:grid-cols-2 gap-8">
              {companyValues.map((value) => (
                <MagicCard className="p-8" key={value.title}>
                  <h3 className="text-xl font-semibold mb-4">{value.title}</h3>
                  <p className="text-muted-foreground">{value.description}</p>
                </MagicCard>
              ))}
            </div>
          </div>
        </section> */}

        {/* Team Image Section */}
        <section className="w-full px-4">
          <div className="mx-auto max-w-4xl">
            <div className="text-center mb-12">
              {/* <h2 className="text-3xl font-bold mb-4">Meet Our Team</h2>
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                The crazy people behind BAML who believe AI development should
                be simple, type-safe, and actually work in production.
              </p> */}
            </div>
            <div className="w-full mb-12">
              <Image
                alt="Our team"
                className="w-full rounded-lg shadow-lg"
                height={1000}
                src="/team.jpg"
                width={1000}
              />
            </div>

            {/* Team Member Grid */}
            <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-8">
              {teamMembers.map((member) => (
                <MagicCard className="p-6 text-center" key={member.name}>
                  <div className="mb-4">
                    <Image
                      alt={member.name}
                      className="w-20 h-20 rounded-full mx-auto object-cover"
                      height={80}
                      src={member.image}
                      width={80}
                    />
                  </div>
                  <h3 className="font-semibold mb-1">{member.name}</h3>
                  <p className="text-sm text-primary mb-3">{member.role}</p>
                  {/* <p className="text-sm text-muted-foreground mb-4">
                    {member.description}
                  </p> */}
                  {member.linkedin && (
                    <Button asChild size="sm" variant="outline">
                      <Link href={member.linkedin} target="_blank">
                        LinkedIn
                      </Link>
                    </Button>
                  )}
                </MagicCard>
              ))}
            </div>
          </div>
        </section>

        {/* Join Us Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-4xl text-center">
            <h2 className="text-3xl font-bold mb-4">Join the Community</h2>
            <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
              Ready to build type-safe AI applications? Join thousands of
              developers who are already using BAML in production.
            </p>
            <div className="flex flex-wrap gap-4 justify-center">
              <Button asChild>
                <Link href="https://github.com/boundaryml/baml">
                  <Github className="mr-2 h-4 w-4" />
                  Star on GitHub
                </Link>
              </Button>
              <Button asChild variant="outline">
                <Link href="https://boundaryml.com/discord">
                  <MessageCircle className="mr-2 h-4 w-4" />
                  Join Discord
                </Link>
              </Button>
              {/* <Button asChild variant="outline">
                <Link href="/play">Try BAML</Link>
              </Button> */}
            </div>
          </div>
        </section>

        <FooterSection />
      </main>
    </div>
  );
}
