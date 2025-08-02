test TestOpenAIShorthand {
  functions [TestOpenAIShorthand]
  args {
    input "Donkey kong and peanut butter"
  }
}

test TestAWS {
  functions [
    TestAws
  ]
  args {
    input "Donkey kong and peanut butter"
  }
}

test TestProvider {
  functions [
    TestAnthropic, TestVertex, PromptTestOpenAI, TestAzure, TestOllama, TestGemini, TestGeminiThinking, TestAws,
    TestAwsInvalidRegion,
    TestOpenAIShorthand,
    TestAnthropicShorthand,
    TestAwsInvalidAccessKey,
    TestAwsInvalidProfile,
    TestAwsInvalidSessionToken
  ]
  args {
    input "Donkey kong and peanut butter"
  }
}

test TestName {
  functions [TestCaching]
  args {
    input #"
In a near-future society where dreams have become a tradable commodity and shared experience, a lonely and socially awkward teenager named Alex discovers they possess a rare and powerful ability to not only view but also manipulate the dreams of others. Initially thrilled by this newfound power, Alex begins subtly altering the dreams of classmates and family members, helping them overcome fears, boost confidence, or experience fantastical adventures. As Alex's skills grow, so does their influence. They start selling premium dream experiences on the black market, crafting intricate and addictive dreamscapes for wealthy clients. However, the line between dream and reality begins to blur for those exposed to Alex's creations. Some clients struggle to differentiate between their true memories and the artificial ones implanted by Alex's dream manipulation.

Complications arise when a mysterious government agency takes notice of Alex's unique abilities. They offer Alex a chance to use their gift for "the greater good," hinting at applications in therapy, criminal rehabilitation, and even national security. Simultaneously, an underground resistance movement reaches out, warning Alex about the dangers of dream manipulation and the potential for mass control and exploitation. Caught between these opposing forces, Alex must navigate a complex web of ethical dilemmas. They grapple with questions of free will, the nature of consciousness, and the responsibility that comes with having power over people's minds. As the consequences of their actions spiral outward, affecting the lives of loved ones and strangers alike, Alex is forced to confront the true nature of their ability and decide how—or if—it should be used.

The story explores themes of identity, the subconscious mind, the ethics of technology, and the power of imagination. It delves into the potential consequences of a world where our most private thoughts and experiences are no longer truly our own, and examines the fine line between helping others and manipulating them for personal gain or a perceived greater good. The narrative further expands on the societal implications of such abilities, questioning the moral boundaries of altering consciousness and the potential for abuse in a world where dreams can be commodified. It challenges the reader to consider the impact of technology on personal autonomy and the ethical responsibilities of those who wield such power.

As Alex's journey unfolds, they encounter various individuals whose lives have been touched by their dream manipulations, each presenting a unique perspective on the ethical quandaries at hand. From a classmate who gains newfound confidence to a wealthy client who becomes addicted to the dreamscapes, the ripple effects of Alex's actions are profound and far-reaching. The government agency's interest in Alex's abilities raises questions about the potential for state control and surveillance, while the resistance movement highlights the dangers of unchecked power and the importance of safeguarding individual freedoms.

Ultimately, Alex's story is one of self-discovery and moral reckoning, as they must decide whether to embrace their abilities for personal gain, align with the government's vision of a controlled utopia, or join the resistance in their fight for freedom and autonomy. The narrative invites readers to reflect on the nature of reality, the boundaries of human experience, and the ethical implications of a world where dreams are no longer private sanctuaries but shared and manipulated commodities. It also explores the psychological impact on Alex, who must deal with the burden of knowing the intimate fears and desires of others, and the isolation that comes from being unable to share their own dreams without altering them.

The story further examines the technological advancements that have made dream manipulation possible, questioning the role of innovation in society and the potential for both progress and peril. It considers the societal divide between those who can afford to buy enhanced dream experiences and those who cannot, highlighting issues of inequality and access. As Alex becomes more entangled in the web of their own making, they must confront the possibility that their actions could lead to unintended consequences, not just for themselves but for the fabric of society as a whole.

In the end, Alex's journey is a cautionary tale about the power of dreams and the responsibilities that come with wielding such influence. It serves as a reminder of the importance of ethical considerations in the face of technological advancement and the need to balance innovation with humanity. The story leaves readers pondering the true cost of a world where dreams are no longer sacred, and the potential for both wonder and danger in the uncharted territories of the mind. But it's also a story about the power of imagination and the potential for change, even in a world where our deepest thoughts are no longer our own. And it's a story about the power of choice, and the importance of fighting for the freedom to dream.

In conclusion, this story is a reflection on the power of dreams and the responsibilities that come with wielding such influence. It serves as a reminder of the importance of ethical considerations in the face of technological advancement and the need to balance innovation with humanity. The story leaves readers pondering the true cost of a world where dreams are no longer sacred, and the potential for both wonder and danger in the uncharted territories of the mind. But it's also a story about the power of imagination and the potential for change, even in a world where our deepest thoughts are no longer our own. And it's a story about the power of choice, and the importance of fighting for the freedom to dream.
    "#
    not_cached #"
      hello world
    "#
  }
}