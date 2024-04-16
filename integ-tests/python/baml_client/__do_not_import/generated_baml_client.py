# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from .clients.client_claude import Claude
from .clients.client_gpt35 import GPT35
from .clients.client_gpt4 import GPT4
from .clients.client_gpt4turbo import GPT4Turbo
from .clients.client_lottery_complexsyntax import Lottery_ComplexSyntax
from .clients.client_lottery_simplesyntax import Lottery_SimpleSyntax
from .clients.client_ollama import Ollama
from .clients.client_resilient_complexsyntax import Resilient_ComplexSyntax
from .clients.client_resilient_simplesyntax import Resilient_SimpleSyntax
from .functions.fx_extractresume import BAMLExtractResume
from .functions.fx_fnclassoptional import BAMLFnClassOptional
from .functions.fx_fnclassoptional2 import BAMLFnClassOptional2
from .functions.fx_fnclassoptionaloutput import BAMLFnClassOptionalOutput
from .functions.fx_fnclassoptionaloutput2 import BAMLFnClassOptionalOutput2
from .functions.fx_fnenumlistoutput import BAMLFnEnumListOutput
from .functions.fx_fnenumoutput import BAMLFnEnumOutput
from .functions.fx_fnnamedargssinglestringoptional import BAMLFnNamedArgsSingleStringOptional
from .functions.fx_fnoutputbool import BAMLFnOutputBool
from .functions.fx_fnoutputclass import BAMLFnOutputClass
from .functions.fx_fnoutputclasslist import BAMLFnOutputClassList
from .functions.fx_fnoutputclasswithenum import BAMLFnOutputClassWithEnum
from .functions.fx_fnoutputstringlist import BAMLFnOutputStringList
from .functions.fx_fnstringoptional import BAMLFnStringOptional
from .functions.fx_fntestaliasedenumoutput import BAMLFnTestAliasedEnumOutput
from .functions.fx_fntestclassalias import BAMLFnTestClassAlias
from .functions.fx_fntestclassoverride import BAMLFnTestClassOverride
from .functions.fx_fntestenumoverride import BAMLFnTestEnumOverride
from .functions.fx_fntestnamedargssingleenum import BAMLFnTestNamedArgsSingleEnum
from .functions.fx_fntestoutputadapter import BAMLFnTestOutputAdapter
from .functions.fx_optionaltest_function import BAMLOptionalTest_Function
from .functions.fx_prompttest import BAMLPromptTest
from .functions.fx_testfnnamedargssinglebool import BAMLTestFnNamedArgsSingleBool
from .functions.fx_testfnnamedargssingleclass import BAMLTestFnNamedArgsSingleClass
from .functions.fx_testfnnamedargssingleenumlist import BAMLTestFnNamedArgsSingleEnumList
from .functions.fx_testfnnamedargssinglefloat import BAMLTestFnNamedArgsSingleFloat
from .functions.fx_testfnnamedargssingleint import BAMLTestFnNamedArgsSingleInt
from .functions.fx_testfnnamedargssinglestring import BAMLTestFnNamedArgsSingleString
from .functions.fx_testfnnamedargssinglestringarray import BAMLTestFnNamedArgsSingleStringArray
from .functions.fx_testfnnamedargssinglestringlist import BAMLTestFnNamedArgsSingleStringList
from .functions.fx_testfnnamedargssyntax import BAMLTestFnNamedArgsSyntax
from .functions.fx_uniontest_function import BAMLUnionTest_Function
from baml_core.otel import add_message_transformer_hook, flush_trace_logs
from baml_core.provider_manager import LLMManager
from baml_core.services import LogSchema
from baml_lib import DeserializerException, baml_init
from typing import Callable, List, Optional


class BAMLClient:
    ExtractResume = BAMLExtractResume
    FnClassOptional = BAMLFnClassOptional
    FnClassOptional2 = BAMLFnClassOptional2
    FnClassOptionalOutput = BAMLFnClassOptionalOutput
    FnClassOptionalOutput2 = BAMLFnClassOptionalOutput2
    FnEnumListOutput = BAMLFnEnumListOutput
    FnEnumOutput = BAMLFnEnumOutput
    FnNamedArgsSingleStringOptional = BAMLFnNamedArgsSingleStringOptional
    FnOutputBool = BAMLFnOutputBool
    FnOutputClass = BAMLFnOutputClass
    FnOutputClassList = BAMLFnOutputClassList
    FnOutputClassWithEnum = BAMLFnOutputClassWithEnum
    FnOutputStringList = BAMLFnOutputStringList
    FnStringOptional = BAMLFnStringOptional
    FnTestAliasedEnumOutput = BAMLFnTestAliasedEnumOutput
    FnTestClassAlias = BAMLFnTestClassAlias
    FnTestClassOverride = BAMLFnTestClassOverride
    FnTestEnumOverride = BAMLFnTestEnumOverride
    FnTestNamedArgsSingleEnum = BAMLFnTestNamedArgsSingleEnum
    FnTestOutputAdapter = BAMLFnTestOutputAdapter
    OptionalTest_Function = BAMLOptionalTest_Function
    PromptTest = BAMLPromptTest
    TestFnNamedArgsSingleBool = BAMLTestFnNamedArgsSingleBool
    TestFnNamedArgsSingleClass = BAMLTestFnNamedArgsSingleClass
    TestFnNamedArgsSingleEnumList = BAMLTestFnNamedArgsSingleEnumList
    TestFnNamedArgsSingleFloat = BAMLTestFnNamedArgsSingleFloat
    TestFnNamedArgsSingleInt = BAMLTestFnNamedArgsSingleInt
    TestFnNamedArgsSingleString = BAMLTestFnNamedArgsSingleString
    TestFnNamedArgsSingleStringArray = BAMLTestFnNamedArgsSingleStringArray
    TestFnNamedArgsSingleStringList = BAMLTestFnNamedArgsSingleStringList
    TestFnNamedArgsSyntax = BAMLTestFnNamedArgsSyntax
    UnionTest_Function = BAMLUnionTest_Function
    Claude = Claude
    GPT35 = GPT35
    GPT4 = GPT4
    GPT4Turbo = GPT4Turbo
    Lottery_ComplexSyntax = Lottery_ComplexSyntax
    Lottery_SimpleSyntax = Lottery_SimpleSyntax
    Ollama = Ollama
    Resilient_ComplexSyntax = Resilient_ComplexSyntax
    Resilient_SimpleSyntax = Resilient_SimpleSyntax

    def __init__(self):
        LLMManager.validate()
        baml_init()

    def configure(
        self,
        project_id: Optional[str] = None,
        secret_key: Optional[str] = None,
        base_url: Optional[str] = None,
        enable_cache: Optional[bool] = None,
        stage: Optional[str] = None,
    ):
        return baml_init(
            project_id=project_id,
            secret_key=secret_key,
            base_url=base_url,
            enable_cache=enable_cache,
            stage=stage,
        )

    def add_before_send_message_hook(self, hook: Callable[[LogSchema], None]):
        add_message_transformer_hook(hook)

    def flush(self):
        flush_trace_logs()


baml = BAMLClient()
