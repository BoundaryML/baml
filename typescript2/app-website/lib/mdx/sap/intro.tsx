import Image, { type StaticImageData } from 'next/image';
import tech00 from './images/0_0.png';
import tech01 from './images/0_1.png';
import tech02 from './images/0_2.png';
import tech10 from './images/1_0.png';
import tech11 from './images/1_1.png';
import tech12 from './images/1_2.png';
import tech20 from './images/2_0.png';
import tech21 from './images/2_1.png';
import tech22 from './images/2_2.png';
import tech30 from './images/3_0.png';
import tech31 from './images/3_1.png';
import tech32 from './images/3_2.png';
import tech40 from './images/4_0.png';
import tech41 from './images/4_1.png';
import tech42 from './images/4_2.png';
import tech50 from './images/5_0.png';
import tech51 from './images/5_1.png';
import tech52 from './images/5_2.png';
import tech60 from './images/6_0.png';
import tech61 from './images/6_1.png';
import tech62 from './images/6_2.png';
import tech70 from './images/7_0.png';
import tech71 from './images/7_1.png';
import tech72 from './images/7_2.png';
import llmHamer from './images/llm-hammer.png';
import sapLlmFix0 from './images/parser/llm-fix/llm-fix-1.svg';
import sapLlmFix1 from './images/parser/llm-fix/llm-fix-2.svg';
import sapLlmFix2 from './images/parser/llm-fix/llm-fix-3.svg';
import sapSchemaFix0 from './images/parser/schema/schema-valid-1.svg';
import sapSchemaFix1 from './images/parser/schema/schema-valid-2.svg';
import sapSchemaFix2 from './images/parser/schema/schema-valid-3.svg';
import sapYapping0 from './images/parser/yapping/yapping-1.svg';
import sapYapping1 from './images/parser/yapping/yapping-2.svg';
import problemDefinition from './images/problem_definition.png';
import modelPic from './images/wrench-model.png';
import parserPic from './images/wrench-parser.png';
import promptPic from './images/wrench-prompt.png';

const TECHNIQUES: StaticImageData[][] = [
  [tech00, tech01, tech02],
  [tech10, tech11, tech12],
  [tech20, tech21, tech22],
  [tech30, tech31, tech32],
  [tech40, tech41, tech42],
  [tech50, tech51, tech52],
  [],
  [tech60, tech61, tech62],
  [tech70, tech71, tech72],
];

export const TechniqueTitle: React.FC<{
  type?: 'prompt' | 'model' | 'parser' | ('prompt' | 'model' | 'parser')[];
  title: string;
  count: number;
  takeaways: {
    score: number | React.ReactNode;
    notes: string | React.ReactNode;
  };
  children?: React.ReactNode;
}> = ({ type, title, count, children, takeaways = { score: 0, notes: '' } }) => {
  const renderTypeBadge = (type: 'prompt' | 'model' | 'parser') => (
    <div
      className={`flex rounded-md px-2 py-1 text-xs font-semibold text-white ${
        type === 'prompt'
          ? 'bg-[#FFBF00]'
          : type === 'model'
            ? 'bg-[#32CEDC]'
            : type === 'parser'
              ? 'bg-[#A547DC]'
              : 'bg-gray-500'
      }`}
    >
      {type === 'prompt' && 'Prompt'}
      {type === 'model' && 'Model'}
      {type === 'parser' && 'Parser'}
    </div>
  );

  return (
    <div className="my-2 flex flex-col gap-1 pt-24">
      <div className="flex flex-row items-center gap-2">
        {type &&
          (Array.isArray(type)
            ? type.map(renderTypeBadge)
            : renderTypeBadge(type))}
        {count === TECHNIQUES.length - 1 ? (
          <div className="items-center text-lg">
            <span className="font-light">BAML&apos;s Technique (Ours):</span>{' '}
            <span className="font-bold">Schema Aligned Parsing (SAP)</span>
          </div>
        ) : (
          <div className="items-center text-lg">
            <span className="font-light">Technique {count}:</span>{' '}
            <span className="font-bold">{title}</span>
          </div>
        )}
      </div>
      <div className="text-sm">{children}</div>
      {(TECHNIQUES[count]?.length ?? 0) > 0 ? (
        <div className="flex flex-col items-start gap-2 md:flex-row">
          <Image
            alt="Technique"
            className="md:max-w-[30%]"
            fill={false}
            src={TECHNIQUES[count]![0]!}
            style={{ objectFit: 'scale-down' }}
          />
          <Image
            alt="Technique"
            className="md:max-w-[30%]"
            fill={false}
            src={TECHNIQUES[count]![1]!}
            style={{ objectFit: 'scale-down' }}
          />
          <Image
            alt="Technique"
            className="md:max-w-[30%]"
            fill={false}
            src={TECHNIQUES[count]![2]!}
            style={{ objectFit: 'scale-down' }}
          />
        </div>
      ) : (
        <Image alt="LLM Hammer" className="w-52" src={llmHamer} />
      )}
      <div>
        <div>
          <b>My Personal Rating:</b> {takeaways?.score ?? 0} / 5
        </div>
        <div>{takeaways?.notes}</div>
      </div>
      {/* <table className="min-w-full divide-y divide-gray-200">
        <tbody className="divide-y divide-gray-200 bg-white">
          <tr>
            <td className="whitespace-nowrap px-6 py-4 text-sm text-gray-800">
              Easy to implement.
            </td>
            <td className="whitespace-nowrap px-6 py-4 text-sm text-gray-800">
              No guarantees. The LLM will likely not listen to your instructions
              some percentage of the time.
            </td>
          </tr>
        </tbody>
      </table> */}
    </div>
  );
};

export const ProblemStatement: React.FC = () => {
  return (
    <>
      <Image alt="Problem Definition" src={problemDefinition} />

      <div className="flex flex-col items-start">
        Given a QUERY, and a SCHEMA, the three things we can do to change the
        likelihood of generating an output are:
        <div className="flex h-12 flex-row items-center gap-2">
          <Image alt="Prompt" className="w-6" src={promptPic} />
          <span>Change how we construct the prompt and render the schema</span>
        </div>
        <div className="flex h-12  flex-row items-center gap-2">
          <Image alt="Prompt" className="w-6" src={modelPic} />
          <span>Change how tokens are generated</span>
        </div>
        <div className="flex h-12  flex-row items-center gap-2">
          <Image alt="Prompt" className="w-6" src={parserPic} />
          <span>
            Change how we parse the output of the model into our desired
            structure
          </span>
        </div>
      </div>
    </>
  );
};

export const WhatIsSAP: React.FC = () => {
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-4 rounded-lg bg-gray-50 p-4 shadow-md">
        <div className="flex flex-row gap-2 font-bold">
          Handling Invalid JSON sequence
        </div>
        <div className="flex flex-row gap-4">
          <div className="flex flex-col items-center gap-2 md:max-w-[30%]">
            <span className="text-sm text-gray-500">Schema</span>
            <Image
              alt="SAP"
              className=" m-0 shadow-sm"
              src={sapLlmFix0 as StaticImageData}
            />
          </div>
          <div className="flex flex-col items-center gap-2 md:max-w-[30%]">
            <span className="text-sm text-gray-500">LLM Response</span>
            <Image
              alt="SAP"
              className="m-0 shadow-sm"
              src={sapLlmFix1 as StaticImageData}
            />
          </div>
          <div className="flex flex-col items-center gap-2 md:max-w-[30%]">
            <span className="text-sm text-gray-500">Post-SAP</span>
            <Image
              alt="SAP"
              className="m-0 shadow-sm"
              src={sapLlmFix2 as StaticImageData}
            />
          </div>
        </div>
        <div className="mt-4 flex flex-col items-start">
          <span className="font-semibold text-gray-700">
            Mistakes by the LLM fixed by SAP:
          </span>
          <ul className="list-inside list-disc text-gray-600">
            <li>Included a comment</li>
            <li>Used a fraction instead of a float</li>
            <li>
              Forgot quotes around{' '}
              <code className="rounded bg-gray-200 p-1">stands_for</code>
            </li>
            <li>
              Didn&apos;t escape the newline or{' '}
              <code className="rounded bg-gray-200 p-1">&quot;</code> within the
              string
            </li>
            <li>Included a trailing comma</li>
          </ul>
        </div>
      </div>
      <div className="flex flex-col gap-4 rounded-lg bg-gray-50 p-4 shadow-md">
        <div className="flex flex-row gap-2 font-bold">
          Handling Invalid JSON sequence
        </div>
        <div className="flex flex-row gap-2">
          <div className="flex flex-col items-center gap-2 md:max-w-[30%]">
            <span className="text-sm text-gray-500">Schema</span>
            <Image
              alt="SAP"
              className="m-0 shadow-sm"
              src={sapSchemaFix0 as StaticImageData}
            />
          </div>
          <div className="flex flex-col items-center gap-2 md:max-w-[30%]">
            <span className="text-sm text-gray-500">LLM Output</span>
            <Image
              alt="SAP"
              className="m-0 shadow-sm"
              src={sapSchemaFix1 as StaticImageData}
            />
          </div>
          <div className="flex flex-col items-center gap-2 md:max-w-[30%]">
            <span className="text-sm text-gray-500">Post-SAP</span>
            <Image
              alt="SAP"
              className="m-0 shadow-sm"
              src={sapSchemaFix2 as StaticImageData}
            />
          </div>
        </div>
        <div className="mt-4 flex flex-col items-start">
          <span className="font-semibold text-gray-700">
            Mistakes by the LLM fixed by SAP:
          </span>
          <ul className="list-inside list-disc text-gray-600">
            <li>
              <code>&quot;Amazon&quot;</code> was returned as a{' '}
              <code>string</code>, but <code>Founder.prior_jobs</code> should be
              a <code>string[]</code>
            </li>
          </ul>
        </div>
      </div>
      <div className="flex flex-col gap-4 rounded-lg bg-gray-50 p-4 shadow-md">
        <div className="flex flex-row gap-2 font-bold">Handling Yapping</div>
        <div className="flex flex-row gap-2">
          <div className="flex flex-col items-center gap-2 md:max-w-[30%]">
            <span className="text-sm text-gray-500">LLM Output</span>
            <Image
              alt="SAP"
              className="m-0 shadow-sm"
              src={sapYapping0 as StaticImageData}
            />
          </div>
          <div className="flex flex-col items-center gap-2 md:max-w-[30%]">
            <span className="text-sm text-gray-500">Post-SAP</span>
            <Image
              alt="SAP"
              className="m-0 shadow-sm"
              src={sapYapping1 as StaticImageData}
            />
          </div>
        </div>
        <div className="mt-4 flex flex-col items-start">
          <span className="font-semibold text-gray-700">
            Mistakes by the LLM fixed by SAP:
          </span>
          <ul className="list-inside list-disc text-gray-600">
            <li>
              Included a lot of prefix and suffix text that is not relevant to
              our desired output, but may be relevant to the LLM to generate the
              output
            </li>
          </ul>
        </div>
      </div>
    </div>
  );
};
