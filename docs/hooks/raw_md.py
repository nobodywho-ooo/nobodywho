def on_post_page(output, page, config):
    src_path = page.file.src_path.replace("\\", "/")
    meta = f'<meta name="docs-src-path" content="{src_path}">'
    return output.replace("</head>", meta + "</head>", 1)
