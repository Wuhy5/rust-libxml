use libxml::parser::Parser;
use libxml::xpath::Context as XPathContext;

/// 解析 HTML 并用 XPath 查询，返回所有匹配节点的文本内容
pub fn html_xpath_query(html: &str, xpath: &str) -> Result<Vec<String>, String> {
    let parser = Parser::default();
    let doc = parser.parse_string(html).map_err(|e| format!("Parse error: {:?}", e))?;
    let ctx = XPathContext::new(&doc).map_err(|_| "XPath context error".to_string())?;
    let nodes = ctx
        .evaluate(xpath)
        .map_err(|_| "XPath evaluate failed".to_string())?
        .get_nodes_as_vec();
    let mut results = Vec::new();
    for node in nodes {
        for child in node.get_child_nodes() {
            if let Some(libxml::tree::NodeType::TextNode) = child.get_type() {
                let text = child.get_content();
                results.push(text.trim().to_string());
            }
        }
    }
    Ok(results)
}

fn main() {
  let html = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>XPath 测试页面</title>
    <style>
        body { font-family: Arial, sans-serif; background: #f7f7f7; margin: 0; padding: 0; }
        .section { background: #fffbe7; padding: 16px; margin: 16px 0; border-radius: 6px; }
        #item-list { list-style: none; padding: 0; }
        #item-list li { background: #e3eaff; margin: 6px 0; padding: 8px 12px; border-radius: 4px; cursor: pointer; }
        #item-list li.selected { background: #a3c9f7; color: #fff; }
        .footer { text-align: center; color: #888; margin: 24px 0 0 0; }
        .external-link { color: #0077cc; text-decoration: none; }
        .external-link:hover { text-decoration: underline; }
    </style>
    <script>
        document.addEventListener('DOMContentLoaded', function() {
            // 点击 li 高亮
            document.querySelectorAll('#item-list li').forEach(function(li) {
                li.addEventListener('click', function() {
                    document.querySelectorAll('#item-list li').forEach(function(item) {
                        item.classList.remove('selected');
                    });
                    this.classList.add('selected');
                    alert('你选择了: ' + this.textContent);
                });
            });
            // 外部链接点击提示
            document.querySelectorAll('.external-link').forEach(function(link) {
                link.addEventListener('click', function(e) {
                    alert('即将跳转到外部链接: ' + this.href);
                });
            });
        });
    </script>
</head>
<body>
    <h1 id="main-title">欢迎来到 XPath 测试页面</h1>
    <div class="section" data-type="intro">
        <p>这是一个用于测试 XPath 表达式的简单 HTML 页面。</p>
        <a href="https://example.com" class="external-link">外部链接</a>
    </div>
    <ul id="item-list">
        <li class="item" data-id="1">项目一</li>
        <li class="item" data-id="2">项目二</li>
        <li class="item" data-id="3">项目三</li>
    </ul>
    <div class="footer">
        <span>版权所有 &copy; 2024</span>
    </div>
</body>
</html>"#;
  let results = html_xpath_query(&html, "//li").expect("xpath");
  for item in &results {
    println!("Found li: {}", item);
  }
  assert!(!results.is_empty());
}
