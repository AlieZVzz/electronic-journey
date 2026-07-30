# Elasticsearch Java 工程师快速上手

> 面向已经熟悉 Java、Maven、HTTP、JSON 和关系型数据库的工程师。
>
> 文档基线：Elasticsearch 9.x、官方 Elasticsearch Java API Client 9.x、Java 17+。示例依赖版本为 `9.3.0`。实际项目中，应让 Java 客户端与服务端保持相同主版本，并尽量对齐次版本。

## 1. 学完后应该具备什么能力

完成本文后，你应该能够：

- 解释索引、文档、字段、映射、分片、副本、倒排索引和近实时搜索；
- 在本机启动 Elasticsearch，并通过 REST API 验证集群；
- 使用官方 Java API Client 连接 Elasticsearch；
- 创建索引和映射，完成文档的增、删、改、查和批量写入；
- 正确区分全文查询、精确查询、过滤、排序、分页和聚合；
- 避免动态映射、深分页、逐条写入、错误分片数等常见问题；
- 知道哪些做法只适合学习环境，哪些是生产环境的基本要求。

### 推荐阅读方式

这份文档既包含入门主线，也保留了以后上线时会用到的工程细节。第一次阅读不需要从头到尾记住所有内容。

| 你的目标 | 建议先读 |
| --- | --- |
| 先理解 Elasticsearch 是什么 | 第 2～4 节 |
| 尽快运行第一个例子 | 第 5～8 节 |
| 学会写搜索功能 | 第 10～12 节 |
| 在 Spring Boot 中接入 | 第 14 节 |
| 准备生产上线 | 第 15～18 节 |
| 遇到问题快速排查 | 第 21 节 |

第一次学习时，可以暂时跳过：

- 分片数如何规划；
- 乐观并发控制；
- PIT 的完整生命周期；
- pipeline aggregation；
- 生产集群容量和恢复策略。

先把“文档放进去，再按条件搜出来”跑通，建立直觉后再回来看这些内容会容易很多。

## 先从一个业务故事开始

假设你正在开发一个电商网站，数据库里有 100 万件商品。用户在搜索框输入：

```text
无线机械键盘
```

产品经理提出这些要求：

1. 商品名和描述中包含相关词语的商品都能搜到；
2. 商品名匹配比描述匹配更重要；
3. 只看“键盘”分类；
4. 只看有货商品；
5. 价格限制在 300～800 元；
6. 最相关的排前面；
7. 左侧显示每个品牌有多少件商品。

如果只用 MySQL，你可能先想到：

```sql
SELECT *
FROM product
WHERE (name LIKE '%无线机械键盘%'
       OR description LIKE '%无线机械键盘%')
  AND category = 'keyboard'
  AND available = true
  AND price BETWEEN 300 AND 800
ORDER BY ...
LIMIT 20;
```

这里很快会遇到几个问题：

- 用户输入“无线键盘”，数据库中的商品写的是“蓝牙机械键盘”，还能不能找到？
- `LIKE '%关键词%'` 很难高效利用普通索引；
- 商品名和描述都命中时，如何计算哪个更相关？
- 如何处理分词、同义词、拼写和多字段权重？
- 如何在同一次搜索中统计品牌、价格区间等筛选项？

Elasticsearch 的工作方式更像：

```text
商品写入时
  -> 预先分析名称和描述
  -> 建立适合搜索的索引结构

用户搜索时
  -> 找到包含相关词项的商品
  -> 应用分类、库存、价格过滤
  -> 计算相关性
  -> 返回商品和聚合统计
```

把它类比成一本书：

- MySQL 更像按“书号、作者、出版时间”等确定字段查目录；
- Elasticsearch 更像先做好全书词语索引，然后回答“哪些页面谈到了某个主题，而且哪一页最相关”。

后续所有概念和代码，都围绕这个商品搜索故事展开。

## 2. Elasticsearch 是什么

Elasticsearch 是基于 Apache Lucene 构建的分布式搜索与分析引擎。应用通过 HTTP/JSON API 写入、检索和聚合 JSON 文档。

典型用途：

- 站内搜索、商品搜索、内容搜索；
- 日志检索与可观测性分析；
- 指标聚合、实时分析；
- 地理位置检索；
- 向量检索和语义搜索。

不适合直接替代关系型数据库的场景：

- 强依赖跨记录事务和复杂约束；
- 频繁执行多表关联；
- 需要严格、即时的一致性读；
- 把 Elasticsearch 当作唯一事实来源，却没有设计备份、恢复和数据重建机制。

常见架构是：

```text
MySQL/PostgreSQL（事实数据）
        │
        ├── 应用双写 / Outbox / CDC / 消息队列
        ▼
Elasticsearch（搜索与分析视图）
```

Elasticsearch 可以作为主存储，但这需要针对一致性、备份、恢复、更新模型和故障处理做专门设计。初学阶段更容易理解的定位是：**Elasticsearch 是面向搜索的派生数据存储**。

一个更具体的例子：

```text
用户在后台修改商品价格
        │
        ▼
MySQL 事务提交成功              <- 真实价格以这里为准
        │
        ▼
消息队列 / CDC 同步
        │
        ▼
Elasticsearch 更新搜索文档      <- 商品列表和搜索使用这里
```

如果同步晚了几秒，搜索页可能短暂显示旧价格；结算时仍应回到交易数据库重新确认。这就是“事实数据”和“搜索视图”的区别。

## 3. 用数据库概念建立直觉

| 关系型数据库 | Elasticsearch | 说明 |
| --- | --- | --- |
| Database / Schema | Cluster 中的命名空间概念 | Elasticsearch 没有完全等价的 Database |
| Table | Index | 文档的逻辑集合 |
| Row | Document | 一条 JSON 文档 |
| Column | Field | JSON 字段 |
| Schema | Mapping | 字段类型及索引方式 |
| Primary key | `_id` | 文档标识 |
| B-Tree 索引 | 倒排索引、BKD Tree、doc values 等 | 根据字段类型采用不同结构 |
| `WHERE` | Query DSL 中的 query/filter | filter 通常不计算相关性分数 |
| `GROUP BY` | Aggregation | 分桶和指标计算 |
| `ORDER BY` | sort | 排序字段通常需要 doc values |

这个类比只用于入门。Elasticsearch 的核心不是“JSON 版数据库”，而是把数据预先组织成适合检索和聚合的结构。

### 同一个需求的写法对比

需求：查询有货的键盘，价格在 300～800 元，名称与“无线机械键盘”相关。

MySQL 写法大致是：

```sql
SELECT id, name, price
FROM product
WHERE available = true
  AND category = 'keyboard'
  AND price BETWEEN 300 AND 800
  AND name LIKE '%无线机械键盘%'
ORDER BY created_at DESC
LIMIT 20;
```

Elasticsearch 写法大致是：

```json
{
  "size": 20,
  "query": {
    "bool": {
      "must": [
        {
          "match": {
            "name": "无线机械键盘"
          }
        }
      ],
      "filter": [
        {
          "term": {
            "available": true
          }
        },
        {
          "term": {
            "category": "keyboard"
          }
        },
        {
          "range": {
            "price": {
              "gte": 300,
              "lte": 800
            }
          }
        }
      ]
    }
  }
}
```

可以先这样理解：

| SQL / Elasticsearch | 人话 |
| --- | --- |
| `match` | “内容大意和这个搜索词相关” |
| `term` | “字段值必须精确等于这个值” |
| `range` | “字段值必须落在这个范围内” |
| `bool.must` | “必须匹配，而且参与相关性计算” |
| `bool.filter` | “必须满足，但不需要计算相关性” |

## 4. 核心概念

### 4.1 Cluster、Node、Index、Shard、Replica

- **Cluster（集群）**：一个或多个节点组成的整体；
- **Node（节点）**：一个运行中的 Elasticsearch 实例；
- **Index（索引）**：一组结构相近的文档；
- **Primary shard（主分片）**：索引数据被水平拆分后的基本单元；
- **Replica shard（副本分片）**：主分片的复制，用于容错并分担读请求。

写入一条文档时，Elasticsearch 根据路由值决定它属于哪个主分片。默认路由值通常是 `_id`。主分片数量创建后不能直接修改；要改变主分片数量，通常需要新建索引并执行 reindex。

学习环境中的单节点集群，如果索引配置了副本，集群可能显示为 `yellow`：主分片正常，但副本无法和主分片放在同一个节点。这不等于数据不可用。

可以把它类比成连锁图书馆：

| Elasticsearch | 图书馆类比 |
| --- | --- |
| Cluster | 整个连锁图书馆系统 |
| Node | 一家具体分馆 |
| Index | “商品资料”这一类藏书 |
| Primary shard | 原始藏书被分到不同分馆 |
| Replica shard | 同一批藏书的备份副本 |

例如 `products` 有 2 个主分片：

```text
products
├── 主分片 0：一部分商品
└── 主分片 1：另一部分商品
```

搜索 `products` 时，Elasticsearch 会把查询发到相关分片，再合并各分片返回的结果。应用通常不需要知道某件商品具体在哪个分片。

为什么不能一开始就创建 1000 个分片？可以把分片理解为“独立的小型搜索引擎”：每个分片都有管理成本。两本书没必要分散到一百家分馆。

### 4.2 Document 与 `_source`

文档是一个 JSON 对象：

```json
{
  "id": "p-1001",
  "name": "Mechanical Keyboard",
  "brand": "Keychron",
  "category": "keyboard",
  "price": 599.00,
  "tags": ["wireless", "hot-swap"],
  "available": true,
  "createdAt": "2026-07-30T10:00:00Z"
}
```

Elasticsearch 通常保存原始 JSON 到 `_source`，同时根据 mapping 为不同字段建立检索结构。

需要区分：

- `_source`：返回给应用的原始文档；
- 倒排索引：支持全文搜索和精确检索；
- doc values：面向排序和聚合的列式结构，通常默认用于非 `text` 字段。

一条文档可以同时有多种“视图”：

```text
原始商品 JSON
    ├── _source：以后返回给 Java 应用
    ├── name 的倒排索引：用于搜“mechanical keyboard”
    ├── category 的精确索引：用于筛选 keyboard
    └── price 的列式数据：用于排序和价格聚合
```

这也是为什么 Elasticsearch 占用的磁盘可能明显大于原始 JSON：它不是只保存了一份 JSON，而是在为不同访问方式准备数据结构。

### 4.3 倒排索引

假设有两条文档：

```text
1: "Java Elasticsearch Guide"
2: "Java Performance Guide"
```

经过分析后，可以抽象为：

```text
java          -> [1, 2]
elasticsearch -> [1]
performance   -> [2]
guide         -> [1, 2]
```

搜索时不需要逐条扫描文档，而是根据词项快速找到包含它的文档。Lucene 还会保存词频、位置等信息，用于相关性评分和短语查询。

再对比一次普通扫描：

```text
没有倒排索引：
搜索 "java"
-> 打开第 1 条文档看看
-> 打开第 2 条文档看看
-> ...
-> 检查全部 100 万条

有倒排索引：
搜索 "java"
-> 直接找到 java 对应的文档编号列表
-> 读取这些文档
```

代价也很直观：写入时要做更多工作，换取查询时更快。因此 Elasticsearch 很适合“读和搜索很多、数据模型相对可规划”的场景。

### 4.4 Mapping

Mapping 决定字段的数据类型，以及字段如何被索引。常见字段类型：

| 类型 | 适用场景 |
| --- | --- |
| `text` | 会被分词的全文内容，如标题、描述 |
| `keyword` | 精确值，如 ID、状态、分类、标签 |
| `long` / `integer` / `double` / `scaled_float` | 数值 |
| `date` | 日期时间 |
| `boolean` | 布尔值 |
| `object` | 普通 JSON 对象 |
| `nested` | 需要保持数组中对象独立关系的对象数组 |
| `geo_point` | 经纬度 |
| `dense_vector` | 向量 |

最重要的入门区别：

```json
{
  "name": {
    "type": "text",
    "fields": {
      "keyword": {
        "type": "keyword",
        "ignore_above": 256
      }
    }
  }
}
```

- `name` 用于全文搜索，例如 `match`；
- `name.keyword` 用于精确匹配、排序和聚合，例如 `term`、`sort`、`terms aggregation`。

不要对 `text` 字段做精确聚合，也不要默认用 `keyword` 承担自然语言全文搜索。

最常用的选择方法：

| 字段示例 | 推荐类型 | 为什么 |
| --- | --- | --- |
| 商品标题 `"无线机械键盘"` | `text` | 用户会输入其中一部分词语搜索 |
| 商品分类 `"keyboard"` | `keyword` | 只做精确筛选 |
| 商品 ID `"p-1001"` | `keyword` | 虽然包含数字，但不是用来计算的 |
| 价格 `599.00` | `scaled_float` / 数值 | 需要范围、排序、聚合 |
| 上架时间 | `date` | 需要时间范围和排序 |
| 是否有货 | `boolean` | 真/假筛选 |
| 标签数组 | `keyword` | 每个标签通常是完整值 |

判断字符串用 `text` 还是 `keyword`，可以问自己两个问题：

```text
用户是要搜“这句话里包含哪些词”？
    -> text

用户是要判断“整个值是否完全相等”？
    -> keyword
```

例如商品名 `"Apple Magic Keyboard"`：

| 查询 | 查哪个字段 | 可能结果 |
| --- | --- | --- |
| 搜索 `magic keyboard` | `name`（text） | 能命中 |
| 筛选完整商品名 | `name.keyword` | 必须完整相等 |
| 按商品名排序 | `name.keyword` | 使用未分词值排序 |

### 4.5 Analyzer

Analyzer 决定文本如何从字符串变成词项。分析过程通常包括：

```text
字符过滤器 -> 分词器 -> Token 过滤器
```

例如：

```text
"The QUICK Brown-Fox"
        ↓ standard analyzer
["the", "quick", "brown", "fox"]
```

索引时 analyzer 和查询时 analyzer 通常应保持语义一致。中文搜索通常需要选择适合中文的分析方案，并通过 `_analyze` API 验证实际分词结果，不能只看配置名称猜测效果。

为什么分词重要？假设商品名是：

```text
无线蓝牙机械键盘
```

不同分析方案可能得到非常不同的结果：

```text
方案 A：["无", "线", "蓝", "牙", "机", "械", "键", "盘"]
方案 B：["无线", "蓝牙", "机械键盘"]
方案 C：["无线蓝牙", "机械", "键盘"]
```

当用户搜索“蓝牙键盘”时，能否命中以及相关性如何，都取决于索引和查询阶段生成了哪些词项。

不要只在配置文件中看 analyzer 名称，要实际试：

```http
POST /products/_analyze
{
  "analyzer": "standard",
  "text": "Wireless Mechanical Keyboard"
}
```

重点观察返回的 token。中文项目也应该准备一组真实业务词，例如品牌名、型号、中英文混写和缩写，逐个验证。

### 4.6 Near Real-Time

Elasticsearch 是近实时搜索系统。写入成功后，文档已经被接受，但通常需要经过 refresh 才能被 search 看见；默认 refresh 周期通常约为 1 秒。

需要区分：

- `GET index/_doc/id`：实时获取文档；
- `_search`：近实时，依赖 refresh；
- `refresh=true`：每次写入后立即刷新，测试方便但会伤害写入性能；
- `refresh=wait_for`：等待后续 refresh，比强制刷新更温和，但仍会增加响应时间。

生产代码不要为了“写完立刻搜到”而无条件在每次写入后强制 refresh。

可以把 refresh 类比成超市换价签：

```text
后台已经把新价格登记到系统
        -> 写入成功

货架价签下一轮统一更新
        -> refresh

顾客在货架上看到新价格
        -> search 可以搜到
```

测试中常见的现象：

```text
第 1 步：写入 p-1001，返回成功
第 2 步：立刻 search，偶尔搜不到
第 3 步：等待约 1 秒或执行 refresh
第 4 步：search 可以搜到
```

这通常不是“数据丢了”，而是 search 的可见性时机不同。

### 4.7 相关性评分

全文查询通常返回 `_score`。默认相关性算法以 BM25 为基础，综合考虑词频、逆文档频率和字段长度等因素。

- `query` 上下文：判断是否匹配，并计算分数；
- `filter` 上下文：只判断是否匹配，不计算分数，更适合状态、范围、权限条件。

经验规则：**影响相关性的条件放 query；只是缩小结果集的条件放 filter。**

举例：

| 条件 | 放哪里 | 原因 |
| --- | --- | --- |
| 商品名包含“机械键盘” | `must` / query | 匹配程度会影响排名 |
| 分类必须是 keyboard | `filter` | 只需要是或不是 |
| 必须有货 | `filter` | 不需要因为“更有货”获得更高分 |
| 价格 300～800 | `filter` | 是范围限制，不是相关性 |
| 包含 wireless 标签可加分 | `should` | 有更好，没有也可以 |

假设有两个结果：

```text
A：名称是“无线机械键盘”
B：名称是“办公键盘”，描述里提到“机械结构”
```

如果商品名权重更高，A 通常应该排在 B 前面。这就是搜索结果不只是“满足 WHERE 条件”，还需要相关性排名。

## 5. 本地启动

### 5.1 前置条件

- Docker Desktop；
- macOS、Linux，或 Windows + WSL；
- Java 17+；
- Maven 3.9+；
- 可选：IDEA、HTTP Client、Kibana Dev Tools。

### 5.2 使用官方本地开发脚本

官方当前推荐使用以下命令快速启动 Elasticsearch 和 Kibana：

```bash
curl -fsSL https://elastic.co/start-local | sh
```

服务地址：

- Elasticsearch：`http://localhost:9200`
- Kibana：`http://localhost:5601`

> 这套配置只适用于本地开发和测试，不应用于生产环境。

脚本会在当前目录生成本地配置。根据脚本输出或生成的 `.env` 文件取得本地 API Key，然后验证：

```bash
export ES_LOCAL_API_KEY="<your-local-api-key>"

curl \
  -H "Authorization: ApiKey ${ES_LOCAL_API_KEY}" \
  http://localhost:9200
```

检查集群健康：

```bash
curl \
  -H "Authorization: ApiKey ${ES_LOCAL_API_KEY}" \
  "http://localhost:9200/_cluster/health?pretty"
```

查看节点和索引：

```bash
curl \
  -H "Authorization: ApiKey ${ES_LOCAL_API_KEY}" \
  "http://localhost:9200/_cat/nodes?v"

curl \
  -H "Authorization: ApiKey ${ES_LOCAL_API_KEY}" \
  "http://localhost:9200/_cat/indices?v"
```

如果使用 Kibana Dev Tools，后续 REST 示例可以直接粘贴执行，不需要写主机、鉴权头或 `curl`。

## 6. 第一个索引

本文使用商品搜索作为贯穿示例。

先只记住一句话：

> **索引是存放一类文档的地方，同时包含这些文档的搜索规则。**

例如：

```text
products 索引
├── p-1001 商品文档
├── p-1002 商品文档
├── p-1003 商品文档
└── 搜索规则：name 要分词、category 要精确匹配、price 是数字……
```

它大致类似 MySQL 的一张 `product` 表，但并不完全相同：

| MySQL | Elasticsearch |
| --- | --- |
| 创建表 `product` | 创建索引 `products` |
| 定义列类型 | 定义 Mapping |
| 插入一行 | 写入一条 Document |
| 主键 `id` | 文档 `_id` |

### 6.1 第一步：创建一个空索引

在 Kibana Dev Tools 中执行：

```http
PUT /products-demo
```

这里：

```text
PUT             表示创建或设置资源
/products-demo  是索引名称
```

成功后会看到类似响应：

```json
{
  "acknowledged": true,
  "shards_acknowledged": true,
  "index": "products-demo"
}
```

现在索引还是空的，就像刚创建了一张还没有数据的表。

查看它：

```http
GET /_cat/indices/products-demo?v
```

现阶段不需要理解响应中的每一列，只要能看到 `products-demo` 即可。

### 6.2 第二步：放入第一件商品

```http
PUT /products-demo/_doc/p-1001
{
  "name": "Wireless Mechanical Keyboard",
  "category": "keyboard",
  "price": 599.0,
  "available": true
}
```

把路径拆开：

```text
products-demo   放到哪个索引
_doc            操作的是文档
p-1001          这条文档的 ID
```

对应到 MySQL，可以粗略理解为：

```sql
INSERT INTO product(id, name, category, price, available)
VALUES ('p-1001', 'Wireless Mechanical Keyboard',
        'keyboard', 599.0, true);
```

按 ID 取回商品：

```http
GET /products-demo/_doc/p-1001
```

到这里，我们没有定义任何字段类型，但 Elasticsearch 仍然接受了文档。原因是它默认启用了 **动态 Mapping**：第一次看到新字段时，会根据字段值猜测类型。

### 6.3 第三步：看看 Elasticsearch 猜了什么

执行：

```http
GET /products-demo/_mapping
```

返回结果会比下面更完整，可以先只看 `properties`：

```json
{
  "products-demo": {
    "mappings": {
      "properties": {
        "available": {
          "type": "boolean"
        },
        "category": {
          "type": "text",
          "fields": {
            "keyword": {
              "type": "keyword"
            }
          }
        },
        "name": {
          "type": "text",
          "fields": {
            "keyword": {
              "type": "keyword"
            }
          }
        },
        "price": {
          "type": "float"
        }
      }
    }
  }
}
```

Mapping 就是 Elasticsearch 眼中的字段说明书：

```text
available -> boolean，真或假
category  -> text + keyword，字符串
name      -> text + keyword，字符串
price     -> float，数字
```

为什么一个字符串同时出现 `text` 和 `keyword`？

以 `name = "Wireless Mechanical Keyboard"` 为例：

```text
name
    -> text
    -> 被拆成 wireless、mechanical、keyboard
    -> 用于搜索“mechanical keyboard”

name.keyword
    -> keyword
    -> 保留完整的 "Wireless Mechanical Keyboard"
    -> 用于完整匹配、排序和聚合
```

### 6.4 第四步：为什么不能一直让 Elasticsearch 猜

动态 Mapping 很适合第一次体验，但业务项目不能完全依赖猜测。

例如我们写错了字段名：

```json
{
  "pirce": 599.0
}
```

Elasticsearch 不知道我们本来想写 `price`，可能会把 `pirce` 当作一个全新的字段。最后同一个索引里同时出现：

```text
price
pirce
```

又例如商品编号是：

```json
{
  "productCode": 100001
}
```

Elasticsearch 可能把它猜成数字。但商品编号只是一个标识，我们通常不会计算：

```text
100001 + 100002
```

它更适合被定义成 `keyword`。

因此，在正式创建业务索引时，我们主动告诉 Elasticsearch 每个字段是什么类型。这份规则就是显式 Mapping。

先删除刚才的练习索引：

```http
DELETE /products-demo
```

这只是在清理本节创建的练习数据，不要在包含重要数据的索引上执行删除。

### 6.5 第五步：创建正式的商品索引

先看简化结构：

```text
创建 products-v1
└── mappings
    └── properties
        ├── name 是 text
        ├── category 是 keyword
        ├── price 是 double
        └── available 是 boolean
```

完整请求如下：

```http
PUT /products-v1
{
  "mappings": {
    "dynamic": "strict",
    "properties": {
      "id": {
        "type": "keyword"
      },
      "name": {
        "type": "text",
        "fields": {
          "keyword": {
            "type": "keyword"
          }
        }
      },
      "description": {
        "type": "text"
      },
      "brand": {
        "type": "keyword"
      },
      "category": {
        "type": "keyword"
      },
      "price": {
        "type": "double"
      },
      "tags": {
        "type": "keyword"
      },
      "available": {
        "type": "boolean"
      },
      "createdAt": {
        "type": "date"
      }
    }
  }
}
```

不要试图一次记住这段 JSON。按从外到内的顺序读：

```text
PUT /products-v1
    创建名为 products-v1 的索引

mappings
    定义字段的搜索规则

properties
    开始列出每个字段

name.type = text
    商品名需要分词搜索

category.type = keyword
    分类需要精确筛选

price.type = double
    价格要做范围查询和排序

createdAt.type = date
    创建时间要按日期处理
```

各字段为什么这样选择：

| 字段 | 类型 | 使用方式 |
| --- | --- | --- |
| `id` | `keyword` | 按完整 ID 查找 |
| `name` | `text` | 搜索名称中的词 |
| `name.keyword` | `keyword` | 商品名完整匹配或排序 |
| `description` | `text` | 搜索描述内容 |
| `brand` | `keyword` | 按品牌筛选和统计 |
| `category` | `keyword` | 按分类筛选 |
| `price` | `double` | 价格范围、排序和统计 |
| `tags` | `keyword` | 按完整标签筛选 |
| `available` | `boolean` | 筛选是否有货 |
| `createdAt` | `date` | 时间范围和排序 |

`dynamic: "strict"` 的意思是：文档中出现 Mapping 没有声明的新字段时，拒绝这条文档。

例如误写：

```json
{
  "pirce": 599.0
}
```

Elasticsearch 会报错，而不是悄悄创建 `pirce` 字段。对字段稳定的业务索引来说，这能更早发现问题。

> 示例用 `double` 简化价格字段。真实金额系统要根据精度要求评估整数分、`scaled_float` 等方案，并且不应把搜索索引当作账本。

### 6.6 验证索引

查看 Mapping：

```http
GET /products-v1/_mapping
```

查看索引是否存在：

```http
HEAD /products-v1
```

返回 HTTP 200 表示存在，404 表示不存在。

此时还没有任何商品：

```http
GET /products-v1/_count
```

应该得到：

```json
{
  "count": 0
}
```

到这里，第一个正式索引就创建完成了。下一章会把商品文档真正写进去。

可以用下面四句话自检：

```text
products-v1 是什么？
-> 存放商品文档的索引。

p-1001 是什么？
-> 一条商品文档的 ID。

Mapping 是什么？
-> 每个字段的类型和搜索方式说明。

为什么不完全依赖自动推断？
-> 自动推断可能选错类型，也发现不了字段拼写错误。
```

### 6.7 可选进阶：给索引起一个稳定的别名

第一次学习时可以先跳过这一小节。它是为了以后修改 Mapping 时更容易切换索引。

当前物理索引叫：

```text
products-v1
```

但我们希望应用永远访问一个稳定名称：

```text
products
```

二者的关系：

```text
Java 应用
    │
    │ 访问 products
    ▼
别名 products
    │
    ▼
真实索引 products-v1
```

创建别名：

```http
POST /_aliases
{
  "actions": [
    {
      "add": {
        "index": "products-v1",
        "alias": "products",
        "is_write_index": true
      }
    }
  ]
}
```

从现在开始，下面两个读取请求访问的是同一批数据：

```http
GET /products-v1/_search
GET /products/_search
```

为什么不让 Java 代码直接使用 `products-v1`？

以后 Mapping 要修改时，通常不能直接改变已有字段的类型。我们可以：

```text
1. 创建 products-v2
2. 把数据复制到 products-v2
3. 验证新索引
4. 把 products 别名从 v1 切到 v2
```

应用仍然访问 `products`，不需要跟着修改代码：

```text
切换前：
products -> products-v1

切换后：
products -> products-v2
```

现阶段只要知道“别名是索引的稳定入口”即可，不需要马上练习 reindex 和别名切换。

## 7. REST API 先走一遍

理解 REST DSL 后，Java Client 的 builder 会更直观。

后续示例使用别名 `products`。如果你跳过了 6.7 的别名步骤，把请求中的 `products` 替换成 `products-v1` 即可：

```text
已创建别名：PUT /products/_doc/p-1001
跳过了别名：PUT /products-v1/_doc/p-1001
```

先看最常见操作：

| 你想做什么 | REST API | 类似数据库操作 |
| --- | --- | --- |
| 放入或覆盖一件商品 | `PUT /products/_doc/p-1001` | `INSERT` 或全量 `UPDATE` |
| 按 ID 取商品 | `GET /products/_doc/p-1001` | 按主键 `SELECT` |
| 修改部分字段 | `POST /products/_update/p-1001` | 局部 `UPDATE` |
| 删除商品 | `DELETE /products/_doc/p-1001` | `DELETE` |
| 按条件搜索 | `POST /products/_search` | `SELECT ... WHERE ...` |
| 一批写入 | `POST /_bulk` | JDBC batch |

### 7.1 新增或全量覆盖

```http
PUT /products/_doc/p-1001
{
  "id": "p-1001",
  "name": "Mechanical Keyboard",
  "description": "Wireless hot-swappable mechanical keyboard",
  "brand": "Keychron",
  "category": "keyboard",
  "price": 599.00,
  "tags": ["wireless", "hot-swap"],
  "available": true,
  "createdAt": "2026-07-30T10:00:00Z"
}
```

相同 `_id` 再次执行 `index` 会覆盖文档。要求“不存在才创建”时使用 create 语义：

```http
PUT /products/_create/p-1001
{
  "id": "p-1001",
  "name": "Mechanical Keyboard"
}
```

成功响应中重点先看：

```json
{
  "_index": "products-v1",
  "_id": "p-1001",
  "_version": 1,
  "result": "created"
}
```

再次用相同 `_id` 执行 index，`result` 通常会变成 `updated`，版本号也会增加。

### 7.2 按 ID 读取

```http
GET /products/_doc/p-1001
```

找到文档时：

```json
{
  "_index": "products-v1",
  "_id": "p-1001",
  "found": true,
  "_source": {
    "id": "p-1001",
    "name": "Mechanical Keyboard",
    "brand": "Keychron",
    "price": 599.00
  }
}
```

应用真正关心的商品数据通常位于 `_source`。

### 7.3 局部更新

```http
POST /products/_update/p-1001
{
  "doc": {
    "price": 569.00,
    "available": true
  }
}
```

Elasticsearch 底层的“更新”仍会生成新的 Lucene 文档并标记旧版本删除，不适合把高频计数器当作普通字段持续更新。

`index` 和 `update` 的区别：

| 操作 | 传入内容 | 结果 |
| --- | --- | --- |
| index | 完整新文档 | 替换原文档 |
| update + `doc` | 要修改的字段 | 其他字段保留 |

例如原文档包含 `name`、`category`、`price`。如果用 index 只传 `price`，其他字段可能不再存在；使用 update 的 `doc` 只修改价格。

### 7.4 删除

```http
DELETE /products/_doc/p-1001
```

### 7.5 搜索

```http
POST /products/_search
{
  "query": {
    "match": {
      "name": "mechanical keyboard"
    }
  }
}
```

简化后的响应结构：

```json
{
  "took": 3,
  "hits": {
    "total": {
      "value": 1,
      "relation": "eq"
    },
    "hits": [
      {
        "_id": "p-1001",
        "_score": 0.87,
        "_source": {
          "name": "Mechanical Keyboard",
          "price": 599.00
        }
      }
    ]
  }
}
```

第一次看搜索响应，只需要认出：

- `took`：服务端执行查询花费的毫秒数，不等于完整网络响应耗时；
- `hits.total`：匹配数量；
- `hits.hits`：当前页结果；
- `_score`：相关性分数；
- `_source`：原始商品数据。

## 8. Java API Client

Java Client 并没有发明另一套查询语言。它只是把 JSON 请求变成了有类型提示的 Java builder。

例如同一个全文查询：

REST JSON：

```json
{
  "query": {
    "match": {
      "name": "mechanical keyboard"
    }
  }
}
```

Java：

```java
client.search(
    request -> request
        .index("products")
        .query(query -> query
            .match(match -> match
                .field("name")
                .query("mechanical keyboard")
            )
        ),
    Product.class
);
```

阅读 Java builder 时，可以从外向内翻译：

```text
client.search                     发起搜索
  request.index("products")       搜 products
  request.query                   查询条件
    query.match                   使用全文匹配
      field("name")               查 name 字段
      query("...")                用户输入的搜索词
  Product.class                   把 _source 转成 Product
```

### 8.1 Maven 依赖

```xml
<properties>
    <maven.compiler.release>17</maven.compiler.release>
    <elasticsearch-java.version>9.3.0</elasticsearch-java.version>
</properties>

<dependencies>
    <dependency>
        <groupId>co.elastic.clients</groupId>
        <artifactId>elasticsearch-java</artifactId>
        <version>${elasticsearch-java.version}</version>
    </dependency>
</dependencies>
```

注意：

- Java Client 9.x 要求 Java 17+；
- 不要在新项目中继续使用旧的 High Level REST Client；
- Spring Boot 项目优先让 Boot BOM 管理 Jackson 等通用依赖，避免手工版本冲突；
- 如果出现 `ClassNotFoundException: jakarta.json.spi.JsonProvider`，检查依赖树是否错误降级了 Jakarta JSON API。

检查依赖树：

```bash
mvn dependency:tree
```

### 8.2 定义领域对象

```java
package com.example.search;

import java.math.BigDecimal;
import java.util.List;

public record Product(
    String id,
    String name,
    String description,
    String brand,
    String category,
    BigDecimal price,
    List<String> tags,
    boolean available,
    String createdAt
) {
}
```

示例把 `createdAt` 保持为 ISO-8601 字符串，避免最小示例额外引入 Java Time 的 Jackson 模块；正式项目可以配置 `jackson-datatype-jsr310` 后使用 `Instant`。示例 Mapping 使用 `double` 简化价格字段；正式项目需要根据业务精度决定是否改用整数分或 `scaled_float`。对金融账务，不应把搜索索引当作账本。

### 8.3 创建客户端

最简 API Key 连接：

```java
package com.example.search;

import co.elastic.clients.elasticsearch.ElasticsearchClient;

public final class ElasticsearchClients {

    private ElasticsearchClients() {
    }

    public static ElasticsearchClient create() {
        String serverUrl = requiredEnv("ELASTICSEARCH_URL");
        String apiKey = requiredEnv("ELASTICSEARCH_API_KEY");

        return ElasticsearchClient.of(builder -> builder
            .host(serverUrl)
            .apiKey(apiKey)
        );
    }

    private static String requiredEnv(String name) {
        String value = System.getenv(name);
        if (value == null || value.isBlank()) {
            throw new IllegalStateException("Missing environment variable: " + name);
        }
        return value;
    }
}
```

使用：

```java
try (ElasticsearchClient client = ElasticsearchClients.create()) {
    boolean connected = client.ping().value();
    System.out.println("connected = " + connected);
}
```

客户端是线程安全的，通常应作为应用级单例复用，不要每个请求创建一个客户端。应用关闭时再关闭它。

自建集群默认启用 TLS 和认证时，应校验 CA 证书或证书指纹：

```java
import co.elastic.clients.elasticsearch.ElasticsearchClient;
import co.elastic.clients.transport.TransportUtils;

import javax.net.ssl.SSLContext;

String fingerprint = System.getenv("ELASTICSEARCH_CA_FINGERPRINT");
SSLContext sslContext = TransportUtils.sslContextFromCaFingerprint(fingerprint);

ElasticsearchClient client = ElasticsearchClient.of(builder -> builder
    .host("https://localhost:9200")
    .usernameAndPassword(
        System.getenv("ELASTICSEARCH_USERNAME"),
        System.getenv("ELASTICSEARCH_PASSWORD")
    )
    .sslContext(sslContext)
);
```

不要关闭证书校验，也不要把用户名、密码、API Key 或证书私钥写进源码和日志。

### 8.4 写入文档

```java
import co.elastic.clients.elasticsearch.core.IndexResponse;

Product product = new Product(
    "p-1001",
    "Mechanical Keyboard",
    "Wireless hot-swappable mechanical keyboard",
    "Keychron",
    "keyboard",
    new BigDecimal("599.00"),
    List.of("wireless", "hot-swap"),
    true,
    "2026-07-30T10:00:00Z"
);

IndexResponse response = client.index(request -> request
    .index("products")
    .id(product.id())
    .document(product)
);

System.out.printf(
    "index=%s id=%s result=%s version=%d%n",
    response.index(),
    response.id(),
    response.result(),
    response.version()
);
```

### 8.5 按 ID 读取

```java
import co.elastic.clients.elasticsearch.core.GetResponse;

GetResponse<Product> response = client.get(
    request -> request
        .index("products")
        .id("p-1001"),
    Product.class
);

if (response.found()) {
    Product product = response.source();
    System.out.println(product);
} else {
    System.out.println("not found");
}
```

`response.source()` 在文档不存在时可能为 `null`，即使使用了非空类型声明，也应先检查 `found()`。

### 8.6 局部更新与 upsert

```java
public record ProductPatch(
    BigDecimal price,
    Boolean available
) {
}
```

局部更新：

```java
ProductPatch patch = new ProductPatch(new BigDecimal("569.00"), true);

client.update(
    request -> request
        .index("products")
        .id("p-1001")
        .doc(patch),
    Product.class
);
```

upsert 表示文档存在时更新，不存在时插入：

```java
client.update(
    request -> request
        .index("products")
        .id(product.id())
        .doc(new ProductPatch(new BigDecimal("569.00"), true))
        .upsert(product),
    Product.class
);
```

如果多个写入者可能并发更新同一文档，应了解基于 `_seq_no` 和 `_primary_term` 的乐观并发控制，避免“最后写入者覆盖”造成数据丢失。

### 8.7 删除文档

```java
client.delete(request -> request
    .index("products")
    .id("p-1001")
);
```

删除索引是高风险操作：

```java
client.indices().delete(request -> request.index("products-v1"));
```

生产环境应限制删除索引权限，并通过别名、快照和变更流程保护数据。

## 9. 批量写入

逐条写入会产生大量网络往返。导入或同步多条数据时使用 Bulk API。

```java
import co.elastic.clients.elasticsearch.core.BulkRequest;
import co.elastic.clients.elasticsearch.core.BulkResponse;
import co.elastic.clients.elasticsearch.core.bulk.BulkResponseItem;

List<Product> products = loadProducts();

BulkRequest.Builder builder = new BulkRequest.Builder();

for (Product product : products) {
    builder.operations(operation -> operation
        .index(index -> index
            .index("products")
            .id(product.id())
            .document(product)
        )
    );
}

BulkResponse response = client.bulk(builder.build());

if (response.errors()) {
    for (BulkResponseItem item : response.items()) {
        if (item.error() != null) {
            System.err.printf(
                "bulk failure: id=%s type=%s reason=%s%n",
                item.id(),
                item.error().type(),
                item.error().reason()
            );
        }
    }
}
```

关键点：HTTP 请求成功不代表 Bulk 中每一项都成功，必须检查 `response.errors()` 和每个失败 item。

持续流式写入可使用 `BulkIngester`。它可以按操作数、字节数或时间自动成批，并提供背压。默认触发条件包括 1000 个操作或 5 MiB 请求体，但生产参数应通过压测确定。

批量写入经验：

- 从较小批次开始，例如每批 500～1000 条；
- 同时观察每批字节数，不能只看文档数；
- 对 `429 Too Many Requests` 使用有上限的指数退避和抖动；
- 只重试可重试错误，不要无限重试 mapping 错误；
- 使用稳定 `_id` 让重试具备幂等性；
- 记录失败文档的业务标识和错误类型，不要把敏感完整文档写入日志；
- 控制并发，吞吐不足不等于应该无限增加线程。

## 10. Query DSL

面对一个搜索条件时，先用下面这张表选择查询：

| 业务问题 | 常用查询 |
| --- | --- |
| 标题是否与用户输入相关？ | `match` |
| 标题和描述哪个字段匹配？ | `multi_match` |
| 分类是否恰好等于 keyboard？ | `term` |
| 价格是否在 300～800？ | `range` |
| 多个条件如何组合？ | `bool` |
| 字段是否存在？ | `exists` |
| 查全部文档 | `match_all` |
| 必须匹配完整短语 | `match_phrase` |

### 10.1 `match`：全文查询

```java
import co.elastic.clients.elasticsearch.core.SearchResponse;
import co.elastic.clients.elasticsearch.core.search.Hit;

SearchResponse<Product> response = client.search(
    request -> request
        .index("products")
        .query(query -> query
            .match(match -> match
                .field("name")
                .query("mechanical keyboard")
            )
        ),
    Product.class
);

for (Hit<Product> hit : response.hits().hits()) {
    System.out.printf(
        "id=%s score=%s product=%s%n",
        hit.id(),
        hit.score(),
        hit.source()
    );
}
```

`match` 会分析输入文本，适用于 `text` 字段。

例如字段内容是：

```text
Wireless Mechanical Keyboard
```

经过分析后，可能形成：

```text
[wireless, mechanical, keyboard]
```

那么这些搜索都有机会命中：

```text
mechanical
keyboard
mechanical keyboard
```

它不是简单判断整个字符串是否相等，而是基于分析后的词项进行全文检索。

### 10.2 `term`：精确查询

```java
.query(query -> query
    .term(term -> term
        .field("category")
        .value("keyboard")
    )
)
```

`term` 不会像 `match` 那样分析输入，适用于 `keyword`、数值、布尔等精确值字段。

典型错误：

```text
对 text 字段使用 term，期待它像全文查询一样工作。
```

如果 `name` 是 `text` + `keyword` 多字段：

- 全文检索：`name`
- 精确匹配：`name.keyword`

对比：

| 字段和值 | 查询 | 结果直觉 |
| --- | --- | --- |
| `category = "keyboard"`（keyword） | `term: keyboard` | 命中 |
| `category = "keyboard"`（keyword） | `term: key` | 不命中 |
| `name = "Mechanical Keyboard"`（text） | `match: keyboard` | 通常命中 |
| `name.keyword = "Mechanical Keyboard"` | `term: "Mechanical Keyboard"` | 命中 |
| `name.keyword = "Mechanical Keyboard"` | `term: "mechanical keyboard"` | 大小写不同，通常不命中 |

一句话记忆：

```text
match 问：“里面谈没谈到这个词？”
term 问：“这个值是不是一模一样？”
```

### 10.3 `bool`：组合查询

需求：

- 名称匹配 `"mechanical keyboard"`；
- 分类必须是 `keyboard`；
- 价格在 300～800；
- 必须有货；
- 标签中最好包含 `wireless`。

先不用代码，把需求画成条件树：

```text
必须：
└── 名称与 mechanical keyboard 相关      -> must

硬性筛选：
├── category = keyboard                 -> filter
├── 300 <= price <= 800                 -> filter
└── available = true                    -> filter

加分项：
└── tags 包含 wireless                   -> should
```

```java
SearchResponse<Product> response = client.search(
    request -> request
        .index("products")
        .query(query -> query
            .bool(bool -> bool
                .must(must -> must
                    .match(match -> match
                        .field("name")
                        .query("mechanical keyboard")
                    )
                )
                .filter(filter -> filter
                    .term(term -> term
                        .field("category")
                        .value("keyboard")
                    )
                )
                .filter(filter -> filter
                    .range(range -> range
                        .number(number -> number
                            .field("price")
                            .gte(300.0)
                            .lte(800.0)
                        )
                    )
                )
                .filter(filter -> filter
                    .term(term -> term
                        .field("available")
                        .value(true)
                    )
                )
                .should(should -> should
                    .term(term -> term
                        .field("tags")
                        .value("wireless")
                    )
                )
            )
        ),
    Product.class
);
```

`bool` 子句：

| 子句 | 含义 | 是否影响分数 |
| --- | --- | --- |
| `must` | 必须匹配 | 是 |
| `should` | 最好匹配，或按规则至少匹配若干个 | 是 |
| `filter` | 必须满足的过滤条件 | 否 |
| `must_not` | 必须不匹配 | 否 |

注意：`should` 是否“可选”受 `minimum_should_match` 以及是否存在 `must`/`filter` 影响。业务要求至少满足一个时，应显式设置：

```java
.minimumShouldMatch("1")
```

可以把 bool 查询类比成招聘：

| bool 子句 | 招聘类比 |
| --- | --- |
| `must` | 必须具备，并影响候选人评分 |
| `filter` | 必须满足的硬门槛，例如工作地点 |
| `should` | 加分项，例如有相关行业经验 |
| `must_not` | 明确排除条件 |

### 10.4 多字段查询

```java
.query(query -> query
    .multiMatch(multiMatch -> multiMatch
        .query("wireless keyboard")
        .fields("name^3", "description", "tags")
    )
)
```

`name^3` 表示提升 `name` 字段权重。相关性调优应基于真实查询和标注数据，不要只凭主观感觉反复调 boost。

### 10.5 排序

```java
import co.elastic.clients.elasticsearch._types.SortOrder;

SearchResponse<Product> response = client.search(
    request -> request
        .index("products")
        .query(query -> query.matchAll(matchAll -> matchAll))
        .sort(sort -> sort
            .field(field -> field
                .field("price")
                .order(SortOrder.Asc)
            )
        )
        .sort(sort -> sort
            .field(field -> field
                .field("id")
                .order(SortOrder.Asc)
            )
        ),
    Product.class
);
```

排序要有稳定的唯一兜底字段，否则分页时同值文档的相对顺序可能不稳定。

不要直接按 `text` 字段排序，应使用 `keyword` 子字段，例如 `name.keyword`。

### 10.6 高亮

```java
SearchResponse<Product> response = client.search(
    request -> request
        .index("products")
        .query(query -> query
            .match(match -> match
                .field("description")
                .query("wireless keyboard")
            )
        )
        .highlight(highlight -> highlight
            .fields("description", field -> field)
        ),
    Product.class
);

for (Hit<Product> hit : response.hits().hits()) {
    List<String> fragments = hit.highlight().getOrDefault(
        "description",
        List.of()
    );
    System.out.println(fragments);
}
```

高亮片段包含标记，送到网页前必须采用安全的渲染策略，避免把不可信内容直接当 HTML 注入。

## 11. 分页

假设搜索命中了 5 万件商品，但页面每次只显示 20 件：

```text
第 1 页：第 1～20 件
第 2 页：第 21～40 件
...
第 500 页：第 9981～10000 件
```

浅分页和深分页的区别，可以类比成排队取号：

```text
from + size：
“请从头数 9980 个人，然后给我后面 20 个。”

search_after：
“我上次拿到的人最后编号是 X，请从 X 后面继续给我 20 个。”
```

### 11.1 浅分页：`from` + `size`

```java
int page = 1;
int pageSize = 20;
int from = page * pageSize;

SearchResponse<Product> response = client.search(
    request -> request
        .index("products")
        .from(from)
        .size(pageSize)
        .query(query -> query.matchAll(matchAll -> matchAll)),
    Product.class
);
```

默认 `index.max_result_window` 为 10000。越深的分页，每个相关分片需要收集和排序“前面所有页 + 当前页”的候选结果，CPU 和内存成本会不断增加。

适合：

- 普通搜索结果的前几十页；
- 用户不会跳到非常后面的页码；
- 需要支持“第 3 页、第 5 页”这种随机跳页。

不适合：

- 导出全部数据；
- 无限滚动到几万条之后；
- 后台任务逐条扫描全索引。

### 11.2 深分页：`search_after`

第一次请求指定稳定排序：

```java
SearchResponse<Product> firstPage = client.search(
    request -> request
        .index("products")
        .size(20)
        .sort(sort -> sort.field(field -> field
            .field("createdAt")
            .order(SortOrder.Desc)
        ))
        .sort(sort -> sort.field(field -> field
            .field("id")
            .order(SortOrder.Asc)
        )),
    Product.class
);
```

从最后一条 hit 取出 `sort()` 值，作为下一页的 `searchAfter(...)`。所有后续请求必须保持相同的 query 和 sort。

如果翻页期间数据持续变化，并且需要一致的结果视图，应使用 PIT（Point in Time）配合 `search_after`。完成后及时关闭 PIT。

`search_after` 的取舍：

| 优点 | 限制 |
| --- | --- |
| 深分页成本更可控 | 不能方便地直接跳到任意页 |
| 很适合“加载更多”和无限滚动 | 必须保持相同 query 和 sort |
| 可以配合 PIT 获得一致视图 | 必须保存上一页最后一条的 sort 值 |

当前官方建议：

- 用户界面的深分页：`search_after` + PIT；
- 不建议再用 Scroll API 实现实时用户请求的深分页；
- Scroll 更偏向批处理场景，但大量数据处理也应优先评估 PIT、slicing、reindex 等合适 API。

## 12. 聚合

聚合类似 SQL 的 `GROUP BY` 和聚合函数。常见类型：

- Bucket aggregation：`terms`、`date_histogram`、`range`；
- Metric aggregation：`avg`、`sum`、`min`、`max`、`cardinality`；
- Pipeline aggregation：对其他聚合结果再次计算。

例如产品经理问：

```text
“搜索结果一共有多少个品牌？每个品牌分别有多少件？”
```

SQL 思路：

```sql
SELECT brand, COUNT(*)
FROM product
WHERE name LIKE '%keyboard%'
GROUP BY brand
ORDER BY COUNT(*) DESC;
```

Elasticsearch 思路：

```json
{
  "size": 0,
  "query": {
    "match": {
      "name": "keyboard"
    }
  },
  "aggs": {
    "by_brand": {
      "terms": {
        "field": "brand"
      }
    }
  }
}
```

可能得到：

```json
{
  "aggregations": {
    "by_brand": {
      "buckets": [
        {"key": "Logitech", "doc_count": 42},
        {"key": "Keychron", "doc_count": 35},
        {"key": "Apple", "doc_count": 18}
      ]
    }
  }
}
```

页面就可以显示：

```text
品牌
├── Logitech (42)
├── Keychron (35)
└── Apple (18)
```

这就是电商搜索页左侧筛选项背后的常见实现方式。

### 12.1 价格直方图

```java
import co.elastic.clients.elasticsearch._types.aggregations.HistogramBucket;

SearchResponse<Void> response = client.search(
    request -> request
        .index("products")
        .size(0)
        .query(query -> query
            .term(term -> term
                .field("available")
                .value(true)
            )
        )
        .aggregations("price_histogram", aggregation -> aggregation
            .histogram(histogram -> histogram
                .field("price")
                .interval(100.0)
            )
        ),
    Void.class
);

List<HistogramBucket> buckets = response.aggregations()
    .get("price_histogram")
    .histogram()
    .buckets()
    .array();

for (HistogramBucket bucket : buckets) {
    System.out.printf(
        "priceStart=%s count=%d%n",
        bucket.key(),
        bucket.docCount()
    );
}
```

只需要聚合结果时设置 `size(0)`，避免返回无用文档。

### 12.2 聚合注意事项

- `terms` 聚合通常作用于 `keyword`，而不是 `text`；
- `cardinality` 是近似去重计数，不等同于数据库精确 `COUNT(DISTINCT ...)`；
- 高基数字段做大规模 `terms` 聚合可能消耗大量内存；
- 分布式聚合存在候选桶截断和误差语义，必须理解 `size`、`shard_size`；
- 分页遍历大量聚合桶时使用 composite aggregation；
- 不要把超大结果集一次性返回给前端。

## 动手练习：从 4 件商品到一个搜索页面

这个练习把 mapping、写入、查询、过滤、排序和聚合串在一起。

### 第一步：准备数据

假设索引中有 4 件商品：

| ID | 名称 | 品牌 | 分类 | 价格 | 有货 |
| --- | --- | --- | --- | ---: | --- |
| p-1001 | Wireless Mechanical Keyboard | Keychron | keyboard | 599 | 是 |
| p-1002 | Wired Mechanical Keyboard | Logitech | keyboard | 399 | 是 |
| p-1003 | Wireless Office Keyboard | Logitech | keyboard | 199 | 是 |
| p-1004 | Wireless Mechanical Keyboard Pro | Keychron | keyboard | 899 | 否 |

使用 Bulk API 一次写入：

```http
POST /_bulk
{"index":{"_index":"products","_id":"p-1001"}}
{"id":"p-1001","name":"Wireless Mechanical Keyboard","description":"Bluetooth hot-swappable keyboard","brand":"Keychron","category":"keyboard","price":599,"tags":["wireless","hot-swap"],"available":true,"createdAt":"2026-07-30T10:00:00Z"}
{"index":{"_index":"products","_id":"p-1002"}}
{"id":"p-1002","name":"Wired Mechanical Keyboard","description":"USB gaming keyboard","brand":"Logitech","category":"keyboard","price":399,"tags":["wired","gaming"],"available":true,"createdAt":"2026-07-29T10:00:00Z"}
{"index":{"_index":"products","_id":"p-1003"}}
{"id":"p-1003","name":"Wireless Office Keyboard","description":"Quiet slim keyboard","brand":"Logitech","category":"keyboard","price":199,"tags":["wireless","office"],"available":true,"createdAt":"2026-07-28T10:00:00Z"}
{"index":{"_index":"products","_id":"p-1004"}}
{"id":"p-1004","name":"Wireless Mechanical Keyboard Pro","description":"Premium aluminum keyboard","brand":"Keychron","category":"keyboard","price":899,"tags":["wireless","premium"],"available":false,"createdAt":"2026-07-27T10:00:00Z"}
```

> Bulk 请求体使用 NDJSON 格式：操作行和数据行交替出现，并且最后一行也要有换行符。

学习环境中执行一次 refresh，确保马上可以搜索：

```http
POST /products/_refresh
```

### 第二步：只有全文搜索

搜索 `"wireless mechanical keyboard"`：

```http
POST /products/_search
{
  "query": {
    "match": {
      "name": "wireless mechanical keyboard"
    }
  }
}
```

直觉上：

- p-1001 三个词都匹配，相关性较高；
- p-1004 三个词也都匹配；
- p-1002 匹配 `mechanical` 和 `keyboard`；
- p-1003 匹配 `wireless` 和 `keyboard`。

这里“匹配”不等于“必须包含全部三个词”。具体行为会受到 operator、minimum_should_match 和 analyzer 等配置影响。

### 第三步：加上业务筛选

要求“有货，并且价格在 300～800”：

```http
POST /products/_search
{
  "query": {
    "bool": {
      "must": [
        {
          "match": {
            "name": "wireless mechanical keyboard"
          }
        }
      ],
      "filter": [
        {
          "term": {
            "available": true
          }
        },
        {
          "range": {
            "price": {
              "gte": 300,
              "lte": 800
            }
          }
        }
      ]
    }
  }
}
```

逐个排除：

```text
p-1001：匹配、有货、599 元       -> 保留
p-1002：部分匹配、有货、399 元   -> 保留
p-1003：匹配但只有 199 元        -> 被价格过滤
p-1004：匹配但无货且 899 元      -> 被库存和价格过滤
```

最终剩下 p-1001 和 p-1002，全文匹配程度再决定它们的先后顺序。

### 第四步：同时返回品牌统计

```http
POST /products/_search
{
  "size": 20,
  "query": {
    "bool": {
      "must": [
        {
          "match": {
            "name": "wireless mechanical keyboard"
          }
        }
      ],
      "filter": [
        {
          "term": {
            "available": true
          }
        },
        {
          "range": {
            "price": {
              "gte": 300,
              "lte": 800
            }
          }
        }
      ]
    }
  },
  "aggs": {
    "by_brand": {
      "terms": {
        "field": "brand"
      }
    }
  }
}
```

页面可以用同一个响应渲染两部分：

```text
商品列表
├── p-1001 Keychron Wireless Mechanical Keyboard
└── p-1002 Logitech Wired Mechanical Keyboard

品牌筛选
├── Keychron (1)
└── Logitech (1)
```

### 第五步：翻译成 Java

```java
SearchResponse<Product> response = client.search(
    request -> request
        .index("products")
        .size(20)
        .query(query -> query
            .bool(bool -> bool
                .must(must -> must
                    .match(match -> match
                        .field("name")
                        .query("wireless mechanical keyboard")
                    )
                )
                .filter(filter -> filter
                    .term(term -> term
                        .field("available")
                        .value(true)
                    )
                )
                .filter(filter -> filter
                    .range(range -> range
                        .number(number -> number
                            .field("price")
                            .gte(300.0)
                            .lte(800.0)
                        )
                    )
                )
            )
        )
        .aggregations("by_brand", aggregation -> aggregation
            .terms(terms -> terms.field("brand"))
        ),
    Product.class
);
```

先把 JSON DSL 调试正确，再翻译成 Java builder，通常比直接在多层 lambda 中试错更容易。

如果你的目标只是“先学会用 Elasticsearch 完成一个 Java 搜索功能”，做到这里已经完成了入门主线。后面的内容可以在遇到对应问题或准备上线时再读。

## 13. 进阶：`object` 与 `nested`

普通对象数组会被扁平化。假设：

```json
{
  "variants": [
    {"color": "red", "size": "S"},
    {"color": "blue", "size": "L"}
  ]
}
```

使用普通 `object` 时，底层可能类似：

```text
variants.color = [red, blue]
variants.size  = [S, L]
```

查询“red 且 L”可能错误命中，因为字段之间的对象边界丢失。

如果必须保持每个数组元素内部的关联，应将 `variants` 映射为 `nested`，并使用 nested query：

```json
{
  "variants": {
    "type": "nested",
    "properties": {
      "color": {"type": "keyword"},
      "size": {"type": "keyword"}
    }
  }
}
```

`nested` 会增加索引和查询成本，不要把所有对象数组都机械地声明为 nested。

## 14. Spring Boot 中怎么选

Java 项目通常有两种入口：

### 14.1 官方 Java API Client

适合：

- 需要直接使用完整 Elasticsearch API；
- 查询复杂；
- 希望紧跟官方 DSL；
- 需要对请求和响应有精确控制。

建议把 `ElasticsearchClient` 注册为单例 Bean，把搜索逻辑封装到专门的 repository/gateway 中，不让业务层到处拼 DSL。

```java
@Configuration
class ElasticsearchConfig {

    @Bean(destroyMethod = "close")
    ElasticsearchClient elasticsearchClient(
        @Value("${elasticsearch.url}") String url,
        @Value("${elasticsearch.api-key}") String apiKey
    ) {
        return ElasticsearchClient.of(builder -> builder
            .host(url)
            .apiKey(apiKey)
        );
    }
}
```

生产项目不要把真实 API Key 直接放在会提交到版本库的配置文件中。

### 14.2 Spring Data Elasticsearch

适合：

- 团队熟悉 Spring Data repository；
- CRUD 和派生查询较多；
- 愿意接受一层抽象和它的版本约束。

注意：

- Spring Data Elasticsearch 不是 Elasticsearch 服务端本身；
- 它有独立版本兼容矩阵，应以所用 Spring Boot 版本的依赖管理为准；
- 复杂查询仍可能需要 NativeQuery 或官方客户端；
- 初学时先理解原生 mapping 和 Query DSL，再使用 repository 抽象，更容易定位问题。

## 15. 进阶：异常与重试

建议区分：

| 类型 | 示例 | 处理建议 |
| --- | --- | --- |
| 业务或请求错误 | mapping 冲突、非法查询、400 | 修复请求，不要盲目重试 |
| 认证授权错误 | 401、403 | 检查凭据与权限，不要重试风暴 |
| 文档不存在 | 404 | 按业务处理 |
| 版本冲突 | 409 | 重新读取、合并或按并发策略重试 |
| 限流 | 429 | 有上限的指数退避 + 抖动 |
| 临时服务错误 | 502、503、504 | 在幂等前提下有限重试 |
| 网络超时 | connect/read timeout | 判断操作是否可能已成功，再决定重试 |

写请求超时后，客户端不知道服务端是否已经完成操作。稳定 `_id` 和幂等设计比“捕获异常后立刻重试”更重要。

日志至少包含：

- 操作类型；
- 索引或别名；
- 业务请求 ID / trace ID；
- 耗时；
- HTTP 状态或 Elasticsearch error type；
- 可安全记录的业务文档 ID。

避免记录 API Key、Authorization 头和包含敏感信息的完整 `_source`。

## 16. 进阶：测试策略

### 16.1 单元测试

把业务条件与客户端调用分离：

```text
SearchCriteria
    -> QueryFactory
    -> Elasticsearch gateway
```

单元测试重点：

- 不同条件生成的 bool 逻辑；
- 空条件处理；
- 排序字段白名单；
- 分页参数边界；
- 业务对象到索引文档的转换。

### 16.2 集成测试

查询语义、mapping、analyzer、nested、聚合等行为依赖真实 Elasticsearch。只 mock Java Client 无法验证它们。

集成测试建议使用与生产相同主版本和次版本的真实容器，覆盖：

- 索引模板或 mapping 能成功创建；
- 样例文档能写入；
- refresh 后查询命中符合预期；
- 中文分词、大小写、标点、同义词等边界；
- bulk 部分失败；
- alias 切换；
- 并发更新；
- 深分页无重复、无遗漏。

每个测试使用唯一索引名，测试结束后删除。不要让测试连接共享的生产集群。

## 17. 上线前检查：生产设计清单

### 17.1 数据建模

- [ ] 每个字段的查询、排序、聚合需求是否明确？
- [ ] `text` 与 `keyword` 是否正确区分？
- [ ] 对象数组是否真的需要 `nested`？
- [ ] 日期格式、时区和数值精度是否统一？
- [ ] 是否限制动态字段，防止 mapping explosion？
- [ ] 是否保留稳定的业务 ID 作为 `_id` 或独立 keyword 字段？

### 17.2 写入链路

- [ ] Elasticsearch 是否是事实来源，还是可重建的搜索视图？
- [ ] 数据库到 Elasticsearch 的同步如何保证最终一致？
- [ ] 是否有重放、补偿和全量重建能力？
- [ ] Bulk 是否检查单项失败？
- [ ] 重试是否幂等、有限、有退避和抖动？
- [ ] 是否避免每条写入强制 refresh？

### 17.3 查询链路

- [ ] 用户输入能否造成昂贵查询？
- [ ] 排序字段是否使用合适的 mapping？
- [ ] 是否限制 `size`、聚合桶数和超时时间？
- [ ] 是否避免深层 `from + size`？
- [ ] 是否为动态排序字段做白名单？
- [ ] 是否对高亮内容做安全渲染？

### 17.4 集群与安全

- [ ] 节点、分片、副本和可用区策略是否匹配数据规模和 SLA？
- [ ] 是否启用 TLS、认证和最小权限？
- [ ] 凭据是否通过密钥管理系统注入并定期轮换？
- [ ] 是否配置快照，并实际演练恢复？
- [ ] 是否监控 JVM、磁盘、分片、线程池、拒绝数、查询延迟和错误率？
- [ ] 是否保留磁盘水位余量，避免磁盘写满？
- [ ] 升级前是否检查 breaking changes 和插件兼容性？

## 18. 常见误区

### 误区 1：创建索引后完全依赖动态 mapping

业务字段一旦被错误推断，修复成本通常是新建索引和 reindex。稳定业务模型优先显式 mapping。

### 误区 2：`term` 和 `match` 可以互换

`match` 面向分析后的全文检索；`term` 面向精确词项。选错查询往往表现为“明明有数据却搜不到”。

### 误区 3：把 `text` 字段用于排序和聚合

通常应使用 `keyword` 子字段。不要为了绕过问题随意打开 `fielddata`，它可能显著增加堆内存压力。

### 误区 4：逐条循环写入

会浪费网络往返和吞吐，应该使用 Bulk 或 BulkIngester。

### 误区 5：Bulk HTTP 200 就代表全部成功

Bulk 允许部分成功。必须检查每个 item。

### 误区 6：无限增大 `index.max_result_window`

这只是把资源问题推向集群。深分页应使用 `search_after`，需要稳定快照时加 PIT。

### 误区 7：分片越多越快

分片本身有内存、文件句柄、调度和集群状态成本。小索引的过多分片通常更慢。

### 误区 8：写入成功后 search 必须立刻可见

Elasticsearch 是近实时搜索。理解 refresh，再根据业务选择等待、一致性提示或专门的读路径。

### 误区 9：通过 wildcard 解决所有模糊搜索

前导通配符可能昂贵。先确认产品需求，再评估 analyzer、ngram、search_as_you_type、wildcard 字段或专门的自动补全方案。

### 误区 10：只做快照，不演练恢复

未经验证的备份不能等同于可恢复能力。恢复演练应该验证耗时、权限、索引、别名和应用查询。

## 19. 建议的 3 天学习路线

### Day 1：概念与 REST

1. 启动本地 Elasticsearch 和 Kibana；
2. 理解 document、mapping、text/keyword、analyzer；
3. 用 REST 完成 CRUD；
4. 练习 match、term、range、bool；
5. 用 `_analyze` 观察分词结果。

### Day 2：Java Client

1. 建立 Java 17 Maven 项目；
2. 创建单例 `ElasticsearchClient`；
3. 实现 CRUD 和 Bulk；
4. 处理不存在、部分失败、超时；
5. 编写真实容器集成测试。

### Day 3：搜索工程化

1. 实现多字段检索、过滤、排序和高亮；
2. 实现 `search_after` 分页；
3. 实现 terms / histogram 聚合；
4. 练习新索引 + reindex + alias 切换；
5. 制定监控、快照、恢复和权限清单。

## 20. 一个最小 Repository 示例

```java
package com.example.search;

import co.elastic.clients.elasticsearch.ElasticsearchClient;
import co.elastic.clients.elasticsearch.core.SearchResponse;
import co.elastic.clients.elasticsearch.core.search.Hit;

import java.io.IOException;
import java.util.List;
import java.util.Objects;

public final class ProductSearchRepository {

    private static final String INDEX = "products";

    private final ElasticsearchClient client;

    public ProductSearchRepository(ElasticsearchClient client) {
        this.client = Objects.requireNonNull(client);
    }

    public void save(Product product) throws IOException {
        client.index(request -> request
            .index(INDEX)
            .id(product.id())
            .document(product)
        );
    }

    public Product findById(String id) throws IOException {
        var response = client.get(
            request -> request.index(INDEX).id(id),
            Product.class
        );
        return response.found() ? response.source() : null;
    }

    public List<Product> search(
        String keyword,
        String category,
        int size
    ) throws IOException {
        int safeSize = Math.max(1, Math.min(size, 100));

        SearchResponse<Product> response = client.search(
            request -> request
                .index(INDEX)
                .size(safeSize)
                .query(query -> query
                    .bool(bool -> bool
                        .must(must -> must
                            .multiMatch(multiMatch -> multiMatch
                                .query(keyword)
                                .fields("name^3", "description")
                            )
                        )
                        .filter(filter -> filter
                            .term(term -> term
                                .field("category")
                                .value(category)
                            )
                        )
                    )
                ),
            Product.class
        );

        return response.hits()
            .hits()
            .stream()
            .map(Hit::source)
            .filter(Objects::nonNull)
            .toList();
    }

    public void deleteById(String id) throws IOException {
        client.delete(request -> request.index(INDEX).id(id));
    }
}
```

真实项目还应补充：

- 参数校验和空条件策略；
- 超时、指标和 trace；
- 业务异常转换；
- 可控的重试；
- 排序与分页；
- 集成测试；
- 敏感数据日志策略。

## 21. 排障速查

### 搜不到刚写入的数据

检查：

1. 是否写入了正确索引或别名；
2. 写响应是否真的成功；
3. 是否尚未 refresh；
4. query 与字段 mapping 是否匹配；
5. 是否使用 `term` 查询了 `text` 字段；
6. analyzer 产生了什么词项。

调试：

```http
GET /products/_doc/p-1001
GET /products/_mapping
POST /products/_refresh

POST /products/_analyze
{
  "field": "name",
  "text": "Mechanical Keyboard"
}
```

### 集群是 yellow

单节点学习环境中，通常是副本无法分配。确认所有主分片正常；生产环境不要简单把副本改为 0 来掩盖节点或分配问题。

```http
GET /_cluster/health
GET /_cat/shards?v
GET /_cluster/allocation/explain
```

### Bulk 有部分失败

检查每个 item 的：

- `_id`；
- status；
- error type；
- error reason。

mapping 错误要修复数据或索引模型；429 才适合有限退避重试。

### 查询变慢

按顺序检查：

1. 请求是否突然扩大 `size` 或聚合桶数；
2. 是否出现 wildcard、regexp、script、nested 等昂贵查询；
3. 是否深分页；
4. 查询命中的分片是否过多；
5. JVM、CPU、磁盘 I/O、线程池拒绝和 GC；
6. 使用 Profile API 定位查询内部耗时；
7. 使用慢日志定位长期问题。

Profile API 本身有开销，不要默认对所有生产请求开启。

## 22. 官方资料

- [Elasticsearch 文档](https://www.elastic.co/docs/reference/elasticsearch)
- [本地开发快速启动](https://www.elastic.co/docs/deploy-manage/deploy/self-managed/local-development-installation-quickstart)
- [Java API Client](https://www.elastic.co/docs/reference/elasticsearch/clients/java)
- [Java Client 安装](https://www.elastic.co/docs/reference/elasticsearch/clients/java/setup/installation)
- [Java Client 连接方式](https://www.elastic.co/docs/reference/elasticsearch/clients/java/setup/connecting)
- [Java Client 常见用法](https://www.elastic.co/docs/reference/elasticsearch/clients/java/usage)
- [Bulk 写入](https://www.elastic.co/docs/reference/elasticsearch/clients/java/usage/indexing-bulk)
- [Mapping](https://www.elastic.co/docs/manage-data/data-store/mapping)
- [`text` 字段](https://www.elastic.co/docs/reference/elasticsearch/mapping-reference/text)
- [`keyword` 字段](https://www.elastic.co/docs/reference/elasticsearch/mapping-reference/keyword)
- [分页与 `search_after`](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/paginate-search-results)
- [聚合](https://www.elastic.co/docs/explore-analyze/query-filter/aggregations)

## 23. 版本说明

本文在 2026-07-30 根据 Elastic 官方文档整理，示例基线为：

```text
Elasticsearch: 9.x
Java API Client: 9.3.0
Java: 17+
```

版本策略：

1. 客户端与服务端必须保持兼容的主版本；
2. 尽量对齐次版本，以便使用对应版本新增 API；
3. patch 版本可以在兼容范围内选择最新修复版本；
4. 升级前阅读 Elasticsearch 和 Java Client 的 release notes；
5. Spring Boot / Spring Data 项目同时检查它们各自的兼容矩阵和 BOM。
