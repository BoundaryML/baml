import baml_py
import pydantic


class PdfWrapper(pydantic.BaseModel):
    content: baml_py.Pdf


def test_model_accepts_pdf_instance():
    pdf = baml_py.Pdf.from_base64("CCC")
    obj = PdfWrapper(content=pdf)
    assert isinstance(obj.content, baml_py.Pdf)


def test_model_validate_pdf_from_dict():
    obj = PdfWrapper.model_validate({"content": {"base64": "CCC"}})
    assert isinstance(obj.content, baml_py.Pdf)


def test_model_dump_pdf_url():
    obj = PdfWrapper(content=baml_py.Pdf.from_url("https://example.com/test.pdf"))
    assert obj.model_dump() == {
        "content": {
            "url": "https://example.com/test.pdf",
            "media_type": "application/pdf",
        }
    }
