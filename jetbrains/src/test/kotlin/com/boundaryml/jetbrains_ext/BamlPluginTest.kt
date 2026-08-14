package com.boundaryml.jetbrains_ext

import com.intellij.ide.highlighter.XmlFileType
import com.intellij.openapi.components.service
import com.intellij.psi.xml.XmlFile
import com.intellij.testFramework.TestDataPath
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import com.intellij.util.PsiErrorElementUtil
import org.jetbrains.plugins.textmate.TextMateBackedFileType
import javax.xml.parsers.DocumentBuilderFactory
import javax.xml.xpath.XPathConstants
import javax.xml.xpath.XPathFactory

@TestDataPath("\$CONTENT_ROOT/src/test/testData")
class BamlPluginTest : BasePlatformTestCase() {

    fun testXMLFile() {
        val psiFile = myFixture.configureByText(XmlFileType.INSTANCE, "<foo>bar</foo>")
        val xmlFile = assertInstanceOf(psiFile, XmlFile::class.java)

        assertFalse(PsiErrorElementUtil.hasErrors(project, xmlFile.virtualFile))

        assertNotNull(xmlFile.rootTag)

        xmlFile.rootTag?.let {
            assertEquals("foo", it.name)
            assertEquals("bar", it.value.text)
        }
    }

    fun testRename() {
        myFixture.testRename("foo.xml", "foo_after.xml", "a2")
    }

    fun testProjectService() {
//        val projectService = project.service<BamlProjectService>()
//
//        assertNotSame(projectService.getRandomNumber(), projectService.getRandomNumber())
    }

    fun testTextMateDescriptorUsesSupportedHandoff() {
        assertInstanceOf(BamlFileType.INSTANCE, TextMateBackedFileType::class.java)

        val pluginDescriptor = requireNotNull(javaClass.classLoader.getResourceAsStream("META-INF/plugin.xml"))
            .use { DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(it) }
        val xpath = XPathFactory.newInstance().newXPath()

        // This element is added by patchPluginXml, proving this test inspects Gradle's packaged descriptor rather than
        // the source XML.
        assertEquals("242", xpath.evaluate("string(/idea-plugin/idea-version/@since-build)", pluginDescriptor))
        assertEquals("261.*", xpath.evaluate("string(/idea-plugin/idea-version/@until-build)", pluginDescriptor))
        assertEquals(
            BamlTextMateBundleProvider::class.java.name,
            xpath.evaluate("string(/idea-plugin/extensions/textmate.bundleProvider/@implementation)", pluginDescriptor),
        )
        assertEquals(
            0.0,
            xpath.evaluate(
                "count(/idea-plugin/extensions/*[starts-with(@implementation, 'org.jetbrains.plugins.textmate.') or starts-with(@implementationClass, 'org.jetbrains.plugins.textmate.')])",
                pluginDescriptor,
                XPathConstants.NUMBER,
            ),
        )
    }

    override fun getTestDataPath() = "src/test/testData/rename"
}
