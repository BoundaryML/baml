# build the index in the how-to guide docs.

import json
import os
import re


def extract_how_to_guides(file_path):
    with open(file_path, "r") as file:
        data = json.load(file)
        navigation = data["navigation"]

        for group in navigation:
            if group["group"] == "How-to Guides":
                return group

        return None


def extract_title(file_path):
    try:
        with open(file_path, "r") as file:
            content = file.read()
            match = re.search(r'^---\ntitle: "(.*?)"\n---', content, re.MULTILINE)
            if match:
                return match.group(1)
            else:
                return None
    except FileNotFoundError:
        return None


def generate_group_content(group):
    group_content = ['<div style={{marginBottom: "20px"}}>']
    group_content.append(f'<h2>{group.get("group", "Untitled Group")}</h2>')
    group_content.append("<ul>")
    for page in group["pages"]:
        title = extract_title(page + ".mdx")  # Assuming .mdx extension
        if title:
            group_content.append(f'<li><a href="/{page}">{title}</a></li>')
    group_content.append("</ul>")
    group_content.append("</div>")
    return group_content


def generate_mdx_index(data):
    pages = data["pages"]

    column1_content, column2_content = [], []
    for i, item in enumerate(pages):
        if isinstance(item, str):
            title = extract_title(item + ".mdx")
            if title:
                content = f'<li><a href="/{item}">{title}</a></li>'
                if i % 2 == 0:
                    column1_content.extend(["<ul>", content, "</ul>"])
                else:
                    column2_content.extend(["<ul>", content, "</ul>"])
        elif isinstance(item, dict):
            group_content = generate_group_content(item)
            if i % 2 == 0:
                column1_content.extend(group_content)
            else:
                column2_content.extend(group_content)

    mdx_content = [
        '<div style={{display: "flex", flexDirection: "row", flexWrap: "wrap"}}>'
    ]
    # Column 1
    mdx_content.append(
        '<div style={{flex: "1 0 50%", maxWidth: "50%", padding: "8px"}}>'
    )
    mdx_content.extend(column1_content)
    mdx_content.append("</div>")
    # Column 2
    mdx_content.append(
        '<div style={{flex: "1 0 50%", maxWidth: "50%", padding: "8px"}}>'
    )
    mdx_content.extend(column2_content)
    mdx_content.append("</div>")
    mdx_content.append("</div>")

    return "\n".join(mdx_content)


if __name__ == "__main__":
    print("Generating mdx")
    data = extract_how_to_guides("mint.json")
    mdx = generate_mdx_index(data)
    print(f"MDX content generated:\n{mdx}")
