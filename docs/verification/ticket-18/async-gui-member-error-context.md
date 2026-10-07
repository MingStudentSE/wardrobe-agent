# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: learning.spec.ts >> 确认记忆修改后的再次推荐使用新意图，未知响应重试保持原键
- Location: tests/learning.spec.ts:29:1

# Error details

```
Test timeout of 30000ms exceeded.
```

```
Error: locator.fill: Test timeout of 30000ms exceeded.
Call log:
  - waiting for getByLabel('新成员名称')
    - locator resolved to <input disabled value="" required="" maxlength="80"/>
    - fill("偏好推荐-1791331781079")
  - attempting fill action
    2 × waiting for element to be visible, enabled and editable
      - element is not enabled
    - retrying fill action
    - waiting 20ms
    - waiting for element to be visible, enabled and editable
    - element is not enabled
  - retrying fill action
    - waiting 100ms
  - element was detached from the DOM, retrying
    - locator resolved to <input disabled value="" required="" maxlength="80"/>
    - fill("偏好推荐-1791331781079")
  - attempting fill action
    2 × waiting for element to be visible, enabled and editable
      - element is not visible
    - retrying fill action
    - waiting 20ms
    2 × waiting for element to be visible, enabled and editable
      - element is not visible
    - retrying fill action
      - waiting 100ms
    59 × waiting for element to be visible, enabled and editable
       - element is not visible
     - retrying fill action
       - waiting 500ms

```

# Page snapshot

```yaml
- generic [ref=f1e3]:
  - complementary [ref=f1e4]:
    - paragraph [ref=f1e5]: 衣橱 / WARDROBE
    - paragraph [ref=f1e6]: 本地空间
    - heading "我的衣橱" [level=1] [ref=f1e7]
    - region "成员档案" [ref=f1e8]:
      - generic [ref=f1e9]:
        - text: 当前成员
        - combobox "当前成员" [ref=f1e10]:
          - option "我" [selected]
          - option "学习验收-1791331776351"
          - option "画像验收-1791331777968"
      - group [ref=f1e11]:
        - generic "管理成员档案" [ref=f1e12] [cursor=pointer]
    - link "单品" [ref=f1e13] [cursor=pointer]:
      - /url: "#collection"
    - link "搭配" [ref=f1e14] [cursor=pointer]:
      - /url: "#lookbook"
    - link "穿着" [ref=f1e15] [cursor=pointer]:
      - /url: "#wears"
    - link "洗护" [ref=f1e16] [cursor=pointer]:
      - /url: "#care"
    - link "推荐" [ref=f1e17] [cursor=pointer]:
      - /url: "#recommendations"
    - link "洞察" [ref=f1e18] [cursor=pointer]:
      - /url: "#insights"
    - link "记忆" [ref=f1e19] [cursor=pointer]:
      - /url: "#learning"
    - paragraph [ref=f1e20]: 整理已有衣物。手工录入随时可用。
  - main [ref=f1e21]:
    - generic [ref=f1e22]:
      - generic [ref=f1e23]:
        - heading "衣物，慢慢整理。" [level=2] [ref=f1e24]
        - paragraph [ref=f1e25]: 衣橱、归档与回收站，保留每件衣物的事实。
      - link "录入单品" [ref=f1e26] [cursor=pointer]:
        - /url: "#entry"
    - generic [ref=f1e27]:
      - region "衣物列表" [ref=f1e28]:
        - generic [ref=f1e29]:
          - generic [ref=f1e30]:
            - text: 搜索衣物
            - textbox "搜索衣物" [ref=f1e31]:
              - /placeholder: 名称、品牌、标签…
          - button "搜索" [ref=f1e32] [cursor=pointer]
          - generic [ref=f1e33]:
            - text: 生命周期
            - combobox "生命周期" [ref=f1e34]:
              - option "衣橱" [selected]
              - option "归档"
              - option "回收站"
          - generic [ref=f1e35]:
            - text: 筛选类别
            - combobox "筛选类别" [ref=f1e36]:
              - option "全部类别" [selected]
              - option "上装"
              - option "下装"
              - option "外套"
              - option "连衣裙"
              - option "连体"
              - option "鞋"
              - option "包"
              - option "配饰"
              - option "内衣"
              - option "运动服"
              - option "其他"
          - generic [ref=f1e37]:
            - text: 筛选季节
            - combobox "筛选季节" [ref=f1e38]:
              - option "全部季节" [selected]
              - option "春"
              - option "夏"
              - option "秋"
              - option "冬"
          - generic [ref=f1e39]:
            - text: 收藏筛选
            - combobox "收藏筛选" [ref=f1e40]:
              - option "全部" [selected]
              - option "已收藏"
              - option "未收藏"
          - generic [ref=f1e41]:
            - text: 颜色筛选
            - combobox "颜色筛选" [ref=f1e42]:
              - option "全部颜色" [selected]
              - option "白"
              - option "黑"
              - option "灰"
              - option "米色"
              - option "棕"
              - option "红"
              - option "橙"
              - option "黄"
              - option "绿"
              - option "蓝"
              - option "紫"
              - option "粉"
              - option "银"
              - option "金"
              - option "多色"
              - option "其他"
          - generic [ref=f1e43]:
            - text: 标签筛选
            - textbox "标签筛选" [ref=f1e44]
          - generic [ref=f1e45]:
            - text: 排序
            - combobox "排序" [ref=f1e46]:
              - option "录入顺序" [selected]
              - option "最近录入"
              - option "名称"
              - option "最近修改"
              - option "价格从低到高"
              - option "价格从高到低"
          - group [ref=f1e47]:
            - generic "更多组合筛选" [ref=f1e48] [cursor=pointer]
            - option "全部小类" [selected]
            - option "衬衫"
            - option "T 恤"
            - option "针织衫"
            - option "卫衣"
            - option "长裤"
            - option "牛仔裤"
            - option "半裙"
            - option "短裤"
            - option "大衣"
            - option "夹克"
            - option "西装外套"
            - option "连衣裙"
            - option "连体衣"
            - option "运动鞋"
            - option "靴"
            - option "凉鞋"
            - option "正装鞋"
            - option "手提包"
            - option "背包"
            - option "首饰"
            - option "帽"
            - option "围巾"
            - option "腰带"
            - option "内衣"
            - option "运动服"
            - option "其他"
          - paragraph [ref=f1e49]: 价格按币种分组，未知价格排最后。
        - generic [ref=f1e50]:
          - article [ref=f1e51]:
            - generic "暂无图片" [ref=f1e52]: 上装
            - heading "分页单品 1" [level=3] [ref=f1e54]
            - paragraph [ref=f1e55]: 季节未设置
            - paragraph [ref=f1e56]: 价格未知
            - button "查看详情" [ref=f1e57] [cursor=pointer]
          - article [ref=f1e58]:
            - generic "暂无图片" [ref=f1e59]: 上装
            - heading "分页单品 2" [level=3] [ref=f1e61]
            - paragraph [ref=f1e62]: 季节未设置
            - paragraph [ref=f1e63]: 价格未知
            - button "查看详情" [ref=f1e64] [cursor=pointer]
          - article [ref=f1e65]:
            - generic "暂无图片" [ref=f1e66]: 上装
            - heading "分页单品 3" [level=3] [ref=f1e68]
            - paragraph [ref=f1e69]: 季节未设置
            - paragraph [ref=f1e70]: 价格未知
            - button "查看详情" [ref=f1e71] [cursor=pointer]
          - article [ref=f1e72]:
            - generic "暂无图片" [ref=f1e73]: 上装
            - heading "分页单品 4" [level=3] [ref=f1e75]
            - paragraph [ref=f1e76]: 季节未设置
            - paragraph [ref=f1e77]: 价格未知
            - button "查看详情" [ref=f1e78] [cursor=pointer]
          - article [ref=f1e79]:
            - generic "暂无图片" [ref=f1e80]: 上装
            - heading "分页单品 5" [level=3] [ref=f1e82]
            - paragraph [ref=f1e83]: 季节未设置
            - paragraph [ref=f1e84]: 价格未知
            - button "查看详情" [ref=f1e85] [cursor=pointer]
          - article [ref=f1e86]:
            - generic "暂无图片" [ref=f1e87]: 上装
            - heading "分页单品 6" [level=3] [ref=f1e89]
            - paragraph [ref=f1e90]: 季节未设置
            - paragraph [ref=f1e91]: 价格未知
            - button "查看详情" [ref=f1e92] [cursor=pointer]
          - article [ref=f1e93]:
            - generic "暂无图片" [ref=f1e94]: 上装
            - heading "分页单品 7" [level=3] [ref=f1e96]
            - paragraph [ref=f1e97]: 季节未设置
            - paragraph [ref=f1e98]: 价格未知
            - button "查看详情" [ref=f1e99] [cursor=pointer]
          - article [ref=f1e100]:
            - generic "暂无图片" [ref=f1e101]: 上装
            - heading "分页单品 8" [level=3] [ref=f1e103]
            - paragraph [ref=f1e104]: 季节未设置
            - paragraph [ref=f1e105]: 价格未知
            - button "查看详情" [ref=f1e106] [cursor=pointer]
          - article [ref=f1e107]:
            - generic "暂无图片" [ref=f1e108]: 上装
            - heading "分页单品 9" [level=3] [ref=f1e110]
            - paragraph [ref=f1e111]: 季节未设置
            - paragraph [ref=f1e112]: 价格未知
            - button "查看详情" [ref=f1e113] [cursor=pointer]
          - article [ref=f1e114]:
            - generic "暂无图片" [ref=f1e115]: 上装
            - heading "分页单品 10" [level=3] [ref=f1e117]
            - paragraph [ref=f1e118]: 季节未设置
            - paragraph [ref=f1e119]: 价格未知
            - button "查看详情" [ref=f1e120] [cursor=pointer]
          - article [ref=f1e121]:
            - generic "暂无图片" [ref=f1e122]: 上装
            - heading "分页单品 11" [level=3] [ref=f1e124]
            - paragraph [ref=f1e125]: 季节未设置
            - paragraph [ref=f1e126]: 价格未知
            - button "查看详情" [ref=f1e127] [cursor=pointer]
          - article [ref=f1e128]:
            - generic "暂无图片" [ref=f1e129]: 上装
            - heading "分页单品 12" [level=3] [ref=f1e131]
            - paragraph [ref=f1e132]: 季节未设置
            - paragraph [ref=f1e133]: 价格未知
            - button "查看详情" [ref=f1e134] [cursor=pointer]
          - article [ref=f1e135]:
            - generic "暂无图片" [ref=f1e136]: 上装
            - heading "分页单品 13" [level=3] [ref=f1e138]
            - paragraph [ref=f1e139]: 季节未设置
            - paragraph [ref=f1e140]: 价格未知
            - button "查看详情" [ref=f1e141] [cursor=pointer]
          - article [ref=f1e142]:
            - generic "暂无图片" [ref=f1e143]: 上装
            - heading "分页单品 14" [level=3] [ref=f1e145]
            - paragraph [ref=f1e146]: 季节未设置
            - paragraph [ref=f1e147]: 价格未知
            - button "查看详情" [ref=f1e148] [cursor=pointer]
          - article [ref=f1e149]:
            - generic "暂无图片" [ref=f1e150]: 上装
            - heading "分页单品 15" [level=3] [ref=f1e152]
            - paragraph [ref=f1e153]: 季节未设置
            - paragraph [ref=f1e154]: 价格未知
            - button "查看详情" [ref=f1e155] [cursor=pointer]
          - article [ref=f1e156]:
            - generic "暂无图片" [ref=f1e157]: 上装
            - heading "分页单品 16" [level=3] [ref=f1e159]
            - paragraph [ref=f1e160]: 季节未设置
            - paragraph [ref=f1e161]: 价格未知
            - button "查看详情" [ref=f1e162] [cursor=pointer]
          - article [ref=f1e163]:
            - generic "暂无图片" [ref=f1e164]: 上装
            - heading "分页单品 17" [level=3] [ref=f1e166]
            - paragraph [ref=f1e167]: 季节未设置
            - paragraph [ref=f1e168]: 价格未知
            - button "查看详情" [ref=f1e169] [cursor=pointer]
          - article [ref=f1e170]:
            - generic "暂无图片" [ref=f1e171]: 上装
            - heading "分页单品 18" [level=3] [ref=f1e173]
            - paragraph [ref=f1e174]: 季节未设置
            - paragraph [ref=f1e175]: 价格未知
            - button "查看详情" [ref=f1e176] [cursor=pointer]
          - article [ref=f1e177]:
            - generic "暂无图片" [ref=f1e178]: 上装
            - heading "分页单品 19" [level=3] [ref=f1e180]
            - paragraph [ref=f1e181]: 季节未设置
            - paragraph [ref=f1e182]: 价格未知
            - button "查看详情" [ref=f1e183] [cursor=pointer]
          - article [ref=f1e184]:
            - generic "暂无图片" [ref=f1e185]: 上装
            - heading "分页单品 20" [level=3] [ref=f1e187]
            - paragraph [ref=f1e188]: 季节未设置
            - paragraph [ref=f1e189]: 价格未知
            - button "查看详情" [ref=f1e190] [cursor=pointer]
          - article [ref=f1e191]:
            - generic "暂无图片" [ref=f1e192]: 上装
            - heading "分页单品 21" [level=3] [ref=f1e194]
            - paragraph [ref=f1e195]: 季节未设置
            - paragraph [ref=f1e196]: 价格未知
            - button "查看详情" [ref=f1e197] [cursor=pointer]
          - article [ref=f1e198]:
            - generic "暂无图片" [ref=f1e199]: 上装
            - heading "分页单品 22" [level=3] [ref=f1e201]
            - paragraph [ref=f1e202]: 季节未设置
            - paragraph [ref=f1e203]: 价格未知
            - button "查看详情" [ref=f1e204] [cursor=pointer]
          - article [ref=f1e205]:
            - generic "暂无图片" [ref=f1e206]: 上装
            - heading "分页单品 23" [level=3] [ref=f1e208]
            - paragraph [ref=f1e209]: 季节未设置
            - paragraph [ref=f1e210]: 价格未知
            - button "查看详情" [ref=f1e211] [cursor=pointer]
          - article [ref=f1e212]:
            - generic "暂无图片" [ref=f1e213]: 上装
            - heading "分页单品 24" [level=3] [ref=f1e215]
            - paragraph [ref=f1e216]: 季节未设置
            - paragraph [ref=f1e217]: 价格未知
            - button "查看详情" [ref=f1e218] [cursor=pointer]
        - button "加载更多" [ref=f1e219] [cursor=pointer]
      - region "录入单品" [ref=f1e220]:
        - heading "添一件单品" [level=2] [ref=f1e221]
        - generic [ref=f1e222]:
          - generic [ref=f1e223]:
            - text: 名称
            - textbox "名称" [ref=f1e224]:
              - /placeholder: 例如：白色棉衬衫
          - generic [ref=f1e225]:
            - text: 类别
            - combobox "类别" [ref=f1e226]:
              - option "上装" [selected]
              - option "下装"
              - option "外套"
              - option "连衣裙"
              - option "连体"
              - option "鞋"
              - option "包"
              - option "配饰"
              - option "内衣"
              - option "运动服"
              - option "其他"
          - group "适用季节 可留空" [ref=f1e227]:
            - generic [ref=f1e229]:
              - generic [ref=f1e230] [cursor=pointer]:
                - checkbox "春" [ref=f1e231]
                - text: 春
              - generic [ref=f1e232] [cursor=pointer]:
                - checkbox "夏" [ref=f1e233]
                - text: 夏
              - generic [ref=f1e234] [cursor=pointer]:
                - checkbox "秋" [ref=f1e235]
                - text: 秋
              - generic [ref=f1e236] [cursor=pointer]:
                - checkbox "冬" [ref=f1e237]
                - text: 冬
          - generic [ref=f1e238]:
            - checkbox "收藏" [ref=f1e239]
            - text: 收藏
          - group [ref=f1e240]:
            - generic "属性与购买信息" [ref=f1e241] [cursor=pointer]
            - option "未设置" [selected]
            - option "衬衫"
            - option "T 恤"
            - option "针织衫"
            - option "卫衣"
          - generic [ref=f1e242]:
            - generic [ref=f1e243]:
              - text: 购入价格
              - spinbutton "购入价格" [ref=f1e244]
            - generic [ref=f1e245]:
              - text: 币种
              - textbox "币种" [ref=f1e246]: CNY
          - paragraph [ref=f1e247]: 未知信息可留空。价格按币种最小单位保存。
          - button "保存单品" [ref=f1e248] [cursor=pointer]
    - generic [ref=f1e249]:
      - generic [ref=f1e250]:
        - generic [ref=f1e251]:
          - heading "搭配 Lookbook" [level=2] [ref=f1e252]
          - paragraph [ref=f1e253]: 保存组合与角色层次。本地拼贴无需模型配置；保存不会记为穿过。今天穿采用保存版本。
        - button "新建搭配" [ref=f1e254] [cursor=pointer]
      - generic [ref=f1e255]:
        - generic [ref=f1e256]:
          - generic [ref=f1e257]:
            - text: 搭配状态
            - combobox "搭配状态" [ref=f1e258]:
              - option "搭配" [selected]
              - option "回收站"
          - list
          - paragraph [ref=f1e259]: 尚无搭配，选一件单品也可以开始。
        - generic [ref=f1e260]:
          - generic [ref=f1e261]:
            - text: 搭配名称
            - textbox "搭配名称" [ref=f1e262]
          - group "选择衣物" [ref=f1e263]:
            - generic [ref=f1e265]:
              - text: 搜索搭配单品
              - textbox "搜索搭配单品" [ref=f1e266]
            - button "搜索衣物" [ref=f1e267] [cursor=pointer]
            - generic [ref=f1e268]:
              - text: 选择搭配单品
              - combobox "选择搭配单品" [ref=f1e269]:
                - option "请选择单品" [selected]
                - option "分页单品 1 · 上装"
                - option "分页单品 2 · 上装"
                - option "分页单品 3 · 上装"
                - option "分页单品 4 · 上装"
                - option "分页单品 5 · 上装"
                - option "分页单品 6 · 上装"
                - option "分页单品 7 · 上装"
                - option "分页单品 8 · 上装"
                - option "分页单品 9 · 上装"
                - option "分页单品 10 · 上装"
                - option "分页单品 11 · 上装"
                - option "分页单品 12 · 上装"
                - option "分页单品 13 · 上装"
                - option "分页单品 14 · 上装"
                - option "分页单品 15 · 上装"
                - option "分页单品 16 · 上装"
                - option "分页单品 17 · 上装"
                - option "分页单品 18 · 上装"
                - option "分页单品 19 · 上装"
                - option "分页单品 20 · 上装"
                - option "分页单品 21 · 上装"
                - option "分页单品 22 · 上装"
                - option "分页单品 23 · 上装"
                - option "分页单品 24 · 上装"
                - option "分页单品 25 · 上装"
            - button "加入搭配" [disabled] [ref=f1e270]
            - list
          - button "保存搭配" [disabled] [ref=f1e271]
    - generic [ref=f1e272]:
      - generic [ref=f1e273]:
        - generic [ref=f1e274]:
          - heading "实际穿着" [level=2] [ref=f1e275]
          - paragraph [ref=f1e276]: 每次确认记一次。同日换装可记多次；次数与当地天数分别计算。
        - button "补记或同日换装" [ref=f1e277] [cursor=pointer]
      - generic [ref=f1e278]:
        - generic [ref=f1e279]:
          - heading "确认一场实际穿着" [level=3] [ref=f1e280]
          - group "日期与真实衣物" [ref=f1e281]:
            - generic [ref=f1e283]:
              - text: 穿着日期
              - textbox "穿着日期" [ref=f1e284]: 2026-10-07
            - generic [ref=f1e285]:
              - text: 穿着时区
              - textbox "穿着时区" [ref=f1e286]: Asia/Shanghai
            - generic [ref=f1e287]:
              - text: 当地时间（可留空）
              - textbox "当地时间（可留空）" [ref=f1e288]
            - paragraph [ref=f1e289]: 时间未知可留空，按当地日期记录，不推断同日先后。
            - generic [ref=f1e290]:
              - text: 选择实际单品
              - combobox "选择实际单品" [ref=f1e291]:
                - option "请选择单品" [selected]
                - option "分页单品 1"
                - option "分页单品 2"
                - option "分页单品 3"
                - option "分页单品 4"
                - option "分页单品 5"
                - option "分页单品 6"
                - option "分页单品 7"
                - option "分页单品 8"
                - option "分页单品 9"
                - option "分页单品 10"
                - option "分页单品 11"
                - option "分页单品 12"
                - option "分页单品 13"
                - option "分页单品 14"
                - option "分页单品 15"
                - option "分页单品 16"
                - option "分页单品 17"
                - option "分页单品 18"
                - option "分页单品 19"
                - option "分页单品 20"
                - option "分页单品 21"
                - option "分页单品 22"
                - option "分页单品 23"
                - option "分页单品 24"
                - option "分页单品 25"
            - button "加入实际穿着" [disabled] [ref=f1e292]
            - generic [ref=f1e293]:
              - text: 穿着备注
              - textbox "穿着备注" [ref=f1e294]
          - button "确认实际穿过" [disabled] [ref=f1e295]
        - generic [ref=f1e296]:
          - heading "有效穿着次数与天数" [level=3] [ref=f1e297]
          - list
          - generic [ref=f1e298]:
            - checkbox "显示已撤销" [ref=f1e299]
            - text: 显示已撤销
          - paragraph [ref=f1e300]: 还没有实际穿着。保存搭配不会增加次数。
    - group [ref=f1e302]:
      - generic "衣橱洞察" [ref=f1e303] [cursor=pointer]
    - generic [ref=f1e304]:
      - generic [ref=f1e305]:
        - generic [ref=f1e306]:
          - heading "从已有衣物，选一套。" [level=2] [ref=f1e307]
          - paragraph [ref=f1e308]: 离线规则生成真实组合。推荐与保存不会记为穿着。
        - button "刷新推荐库存" [ref=f1e309] [cursor=pointer]
      - group [ref=f1e310]:
        - generic "天气地点与缓存" [ref=f1e311] [cursor=pointer]
      - generic [ref=f1e312]:
        - generic [ref=f1e313]:
          - generic [ref=f1e314]:
            - text: 推荐来源
            - combobox "推荐来源" [ref=f1e315]:
              - option "离线规则" [selected]
              - option "文本模型组织候选"
          - generic [ref=f1e316]:
            - text: 推荐数量
            - combobox "推荐数量" [ref=f1e317]:
              - option "1"
              - option "2"
              - option "3" [selected]
              - option "4"
              - option "5"
          - generic [ref=f1e318]:
            - text: 搭配目标
            - combobox "搭配目标" [ref=f1e319]:
              - option "平衡选择" [selected]
              - option "少穿优先"
              - option "收藏优先"
          - generic [ref=f1e320]:
            - text: 搭配场合
            - combobox "搭配场合" [ref=f1e321]:
              - option "日常" [selected]
              - option "工作"
              - option "正式"
              - option "运动"
          - generic [ref=f1e322]:
            - text: 推荐时区
            - textbox "推荐时区" [ref=f1e323]: Asia/Shanghai
          - generic [ref=f1e324]:
            - text: 天气参考
            - combobox "天气参考" [ref=f1e325]:
              - option "关闭天气" [selected]
              - option "手动温度"
              - option "配置地点天气缓存"
        - generic [ref=f1e326]:
          - checkbox "尽量避开最近七个本地自然日实际穿过的整套组合" [checked] [ref=f1e327]
          - text: 尽量避开最近七个本地自然日实际穿过的整套组合
        - paragraph [ref=f1e328]: 固定与排除是硬条件。待洗、归档和回收站单品自动排除；固定不可用时会说明冲突。场合依据已记录标签排序，不猜测风格。
        - group [ref=f1e329]:
          - generic "选择固定、排除与库存优先" [ref=f1e330] [cursor=pointer]
        - button "生成离线搭配" [ref=f1e331] [cursor=pointer]
      - generic [ref=f1e333]:
        - heading "反馈历史" [level=3] [ref=f1e334]
        - generic [ref=f1e335]:
          - checkbox "包含已撤销反馈" [ref=f1e336]
          - text: 包含已撤销反馈
        - button "刷新反馈历史" [ref=f1e337] [cursor=pointer]
        - paragraph [ref=f1e338]: 还没有反馈记录。
        - list
      - heading "本地推荐记录" [level=3] [ref=f1e339]
      - paragraph [ref=f1e340]: 还没有推荐记录。
    - generic [ref=f1e341]:
      - heading "偏好，由你决定。" [level=2] [ref=f1e342]
      - paragraph [ref=f1e343]: 明确偏好与推断画像分开。记忆只影响推荐排序，固定、排除和洗护可用性始终优先；不会改动事实或尺寸。
      - button "查看偏好与记忆" [ref=f1e344] [cursor=pointer]
      - button "查询或继续重算任务" [ref=f1e345] [cursor=pointer]
    - generic [ref=f1e346]:
      - generic [ref=f1e347]:
        - generic [ref=f1e348]:
          - heading "洗护" [level=2] [ref=f1e349]
          - paragraph [ref=f1e350]: 待洗独立于生命周期。清洗或干洗重新计次，维护保留计数。
        - button "刷新洗护" [ref=f1e351] [cursor=pointer]
      - generic [ref=f1e352]:
        - generic [ref=f1e353]:
          - heading "待洗列表" [level=3] [ref=f1e354]
          - paragraph [ref=f1e355]: 没有待洗单品。
          - paragraph [ref=f1e356]: 可查询提醒与推荐排除原因；本页面不自动发送通知。
        - generic [ref=f1e357]:
          - generic [ref=f1e358]:
            - text: 选择洗护单品
            - combobox "选择洗护单品" [ref=f1e359]:
              - option "选择单品" [selected]
              - option "分页单品 1"
              - option "分页单品 2"
              - option "分页单品 3"
              - option "分页单品 4"
              - option "分页单品 5"
              - option "分页单品 6"
              - option "分页单品 7"
              - option "分页单品 8"
              - option "分页单品 9"
              - option "分页单品 10"
              - option "分页单品 11"
              - option "分页单品 12"
              - option "分页单品 13"
              - option "分页单品 14"
              - option "分页单品 15"
              - option "分页单品 16"
              - option "分页单品 17"
              - option "分页单品 18"
              - option "分页单品 19"
              - option "分页单品 20"
              - option "分页单品 21"
              - option "分页单品 22"
              - option "分页单品 23"
              - option "分页单品 24"
              - option "分页单品 25"
          - generic [ref=f1e360]:
            - heading "记录实际洗护" [level=3] [ref=f1e361]
            - generic [ref=f1e362]:
              - text: 洗护类型
              - combobox "洗护类型" [ref=f1e363]:
                - option "清洗" [selected]
                - option "干洗"
                - option "维护"
                - option "标记待洗"
                - option "解除手动待洗"
            - generic [ref=f1e364]:
              - generic [ref=f1e365]:
                - text: 洗护日期
                - textbox "洗护日期" [ref=f1e366]: 2026-10-07
              - generic [ref=f1e367]:
                - text: 洗护时刻（可不填）
                - textbox "洗护时刻（可不填）" [ref=f1e368]
              - generic [ref=f1e369]:
                - text: 洗护时区
                - textbox "洗护时区" [ref=f1e370]: Asia/Shanghai
            - paragraph [ref=f1e371]: 时刻不填只表示日期，以当地中午排序。同刻先穿着再洗护，其余按事件标识；仅知日期的同日先后不确定，可更正时刻。清洗/干洗解除此前手动待洗，维护不解除。
            - generic [ref=f1e372]:
              - text: 洗护备注
              - textbox "洗护备注" [ref=f1e373]
            - button "确认记录洗护" [disabled] [ref=f1e374]
      - heading "洗护历史" [level=3] [ref=f1e375]
      - paragraph [ref=f1e376]: 还没有洗护记录。
    - region "批量录入" [ref=f1e377]:
      - generic [ref=f1e378]:
        - generic [ref=f1e379]:
          - heading "批量录入" [level=2] [ref=f1e380]
          - paragraph [ref=f1e381]: 每文件一个草稿，可归并多图。原图保留在本设备，每批最多 100 个文件。取消保留原图，已入库单品不受影响。
        - button "选择批量图片" [ref=f1e382] [cursor=pointer]
    - region "模型设置" [ref=f1e383]:
      - group [ref=f1e384]:
        - generic "模型设置" [ref=f1e385] [cursor=pointer]
        - option "添加模型" [selected]
        - option "文本与视觉兼容接口" [selected]
        - option "Seedream 专用图片接口"
        - option "API key" [selected]
        - option "无认证（本地模型）"
    - region "客户端授权" [ref=f1e386]:
      - group [ref=f1e387]:
        - generic "客户端授权" [ref=f1e388] [cursor=pointer]
        - option "限定操作" [selected]
        - option "完整业务授权"
    - region "加密空间设置" [ref=f1e389]:
      - group [ref=f1e390]:
        - generic "加密空间与设备" [ref=f1e391] [cursor=pointer]
    - group [ref=f1e393]:
      - generic "备份与数据目录" [ref=f1e394] [cursor=pointer]
```

# Test source

```ts
  1  | import {test,expect} from '@playwright/test';
  2  | test('显式偏好可查看编辑删除，重置与完整重建明确且不写穿着',async({page})=>{
  3  |  await page.goto('/');await page.getByText('管理成员档案',{exact:true}).click();const name=`学习验收-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const member=await choice.getByRole('option',{name,exact:true}).getAttribute('value');await expect(choice).toHaveValue(member!);
  4  |  try{
  5  |  await expect(page.getByRole('link',{name:'记忆',exact:true})).toBeVisible();await page.getByRole('link',{name:'记忆',exact:true}).click();const panel=page.locator('#learning');await panel.getByRole('button',{name:'查看偏好与记忆',exact:true}).click();
  6  |  await panel.getByLabel('偏好维度').selectOption('color');await panel.getByLabel('偏好值').fill('blue');await panel.getByLabel('偏好权重').fill('4');await panel.getByLabel('偏好说明').fill('明确喜欢蓝色');await panel.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(panel.getByRole('status')).toContainText('明确偏好已保存');
  7  |  const explicit=panel.locator('.learning-explicit');await expect(explicit).toContainText('blue');await explicit.getByRole('button',{name:'编辑',exact:true}).click();await panel.getByLabel('偏好权重').fill('-2');await panel.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(explicit).toContainText('-2');
  8  |  await page.reload();await page.getByRole('link',{name:'记忆',exact:true}).click();await panel.getByRole('button',{name:'查看偏好与记忆',exact:true}).click();await expect(explicit).toContainText('-2');
  9  |  await explicit.getByRole('button',{name:'删除',exact:true}).click();await panel.getByLabel('确认删除此记忆').check();await panel.getByRole('button',{name:'确认删除记忆',exact:true}).click();await expect(explicit).not.toContainText('blue');
  10 |  await panel.getByRole('button',{name:'按当前边界重算',exact:true}).click();await expect(panel.getByRole('status')).toContainText('重算已完成');await panel.getByText('重置与完整重建',{exact:true}).click();await panel.getByLabel('确认重置推断画像').check();await panel.getByRole('button',{name:'重置推断画像',exact:true}).click();await expect(panel.getByRole('status')).toContainText('重置已完成');
  11 |  for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await panel.screenshot({path:`../../docs/verification/ticket-18/learning-${width}.png`});}
  12 |  const response=await page.request.post('/__wardrobe_dev_api',{data:{operation:'wear.list',input:{member_id:member}}});expect((await response.json()).result.items).toEqual([]);
  13 |  }finally{await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});}
  14 | });
  15 | test('真实事件画像可读来源、修正排序、删除不复活，显式完整重建保留事实',async({page})=>{
  16 |  await page.goto('/');await page.getByText('管理成员档案',{exact:true}).click();const name=`画像验收-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const member=await choice.getByRole('option',{name,exact:true}).getAttribute('value');await expect(choice).toHaveValue(member!);
  17 |  const call=async(operation:string,input:object={})=>{const r=await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:member,...input}}});const body=await r.json();if(body.error)throw new Error(body.error.message);return body.result;};
  18 |  try{
  19 |  const red=await call('clothing.create',{name:'红上衣',category:'top',color:['red']});const blue=await call('clothing.create',{name:'蓝上衣',category:'top',color:['blue']});for(const category of ['bottom','shoes'])await call('clothing.create',{name:category,category});
  20 |  const wear=await call('wear.record',{operation_key:'actualwearfixture',clothing_ids:[blue.id],local_date:'2020-01-01',timezone:'UTC'});const facts=await call('wear.list');
  21 |  await page.getByRole('link',{name:'记忆',exact:true}).click();const panel=page.locator('#learning');await panel.getByRole('button',{name:'查看偏好与记忆',exact:true}).click();await panel.getByRole('button',{name:'按当前边界重算',exact:true}).click();const inferred=panel.locator('.learning-inferred');const row=inferred.getByRole('listitem').filter({hasText:'颜色 · blue'});await expect(row).toContainText('权重 1');await row.getByText('来源事件 ID',{exact:true}).click();await expect(row).toContainText(wear.id);for(const width of [320,1280]){await page.setViewportSize({width,height:1000});await panel.screenshot({path:`../../docs/verification/ticket-18/learning-sources-${width}.png`});}await page.setViewportSize({width:1280,height:1000});
  22 |  await row.getByRole('button',{name:'修正为明确偏好',exact:true}).click();await panel.getByLabel('偏好权重').fill('5');await panel.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(panel.locator('.learning-explicit')).toContainText('权重 5');
  23 |  const preferred=await call('recommendation.generate',{operation_key:'preferblue',timezone:'UTC',limit:1});expect(preferred.proposals[0].items.some((v:any)=>v.clothing_id===blue.id)).toBeTruthy();
  24 |  await row.getByRole('button',{name:'删除',exact:true}).click();await panel.getByLabel('确认删除此记忆').check();await panel.getByRole('button',{name:'确认删除记忆',exact:true}).click();await panel.getByRole('button',{name:'按当前边界重算',exact:true}).click();await expect(inferred).not.toContainText('颜色 · blue');await expect(panel.locator('.learning-explicit')).not.toContainText('颜色 · blue');
  25 |  await panel.getByText('重置与完整重建',{exact:true}).click();await expect(panel.getByRole('button',{name:'完整重建画像',exact:true})).toBeDisabled();await panel.getByLabel('确认重新纳入此前全部事实').check();await panel.getByRole('button',{name:'完整重建画像',exact:true}).click();await expect(row).toContainText('权重 1');expect(await call('wear.list')).toEqual(facts);expect((await call('clothing.get',{id:red.id})).color).toEqual(['red']);
  26 |  await panel.getByRole('button',{name:'查看修改历史',exact:true}).click();await expect(panel).toContainText('learning.delete');
  27 |  }finally{await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});}
  28 | });
  29 | test('确认记忆修改后的再次推荐使用新意图，未知响应重试保持原键',async({page})=>{
> 30 |  await page.goto('/');await page.getByText('管理成员档案',{exact:true}).click();const name=`偏好推荐-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const member=await choice.getByRole('option',{name,exact:true}).getAttribute('value');await expect(choice).toHaveValue(member!);
     |                                                                                                                                          ^ Error: locator.fill: Test timeout of 30000ms exceeded.
  31 |  const call=async(operation:string,input:object={})=>{const body=await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:member,...input}}})).json();if(body.error)throw new Error(body.error.message);return body.result;};
  32 |  try{
  33 |  for(const [name,category,color] of [['先创建红上衣','top','red'],['后创建蓝上衣','top','blue'],['偏好裤','bottom','black'],['偏好鞋','shoes','black']])await call('clothing.create',{name,category,color:[color]});
  34 |  await page.getByRole('link',{name:'推荐',exact:true}).click();const rec=page.locator('#recommendations');await rec.getByRole('button',{name:'刷新推荐库存',exact:true}).click();await rec.getByLabel('推荐数量').selectOption('1');await rec.getByRole('button',{name:'生成离线搭配',exact:true}).click();await expect(rec.locator('.recommendation-proposal')).toContainText('先创建红上衣');
  35 |  await page.getByRole('link',{name:'记忆',exact:true}).click();const panel=page.locator('#learning');await panel.getByRole('button',{name:'查看偏好与记忆',exact:true}).click();await panel.getByLabel('偏好值').fill('blue');await panel.getByLabel('偏好权重').fill('5');
  36 |  const operationKeys:string[]=[];await page.route('**/__wardrobe_dev_api',async route=>{const data=route.request().postDataJSON();if(data.operation==='learning.set'){operationKeys.push(data.input.operation_key);const response=await route.fetch();if(operationKeys.length===1){await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'偏好已记录，响应丢失'}})});return;}await route.fulfill({response});return;}await route.continue();});
  37 |  await panel.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(panel.getByRole('alert')).toContainText('响应丢失');await panel.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(panel.getByRole('status')).toContainText('明确偏好已保存');expect(operationKeys).toHaveLength(2);expect(operationKeys[0]).toBe(operationKeys[1]);expect((await call('learning.get')).explicit).toHaveLength(1);
  38 |  await page.getByRole('link',{name:'推荐',exact:true}).click();await rec.getByRole('button',{name:'生成离线搭配',exact:true}).click();await expect(rec.locator('.recommendation-proposal')).toContainText('后创建蓝上衣');expect((await call('wear.list')).items).toEqual([]);
  39 |  }finally{await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});}
  40 | });
  41 | test('学习重算立即显示持久任务，重开可查询并明确取消而不发布画像',async({page})=>{
  42 |  await page.goto('/');await page.getByText('管理成员档案',{exact:true}).click();const name=`异步学习-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const member=await choice.getByRole('option',{name,exact:true}).getAttribute('value');await expect(choice).toHaveValue(member!);
  43 |  let release!:()=>void;const gate=new Promise<void>(resolve=>release=resolve);let workers=0;
  44 |  await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='learning.worker.run'){workers++;await gate;}await route.continue();});
  45 |  try{
  46 |  await page.getByRole('link',{name:'记忆',exact:true}).click();const panel=page.locator('#learning');await panel.getByRole('button',{name:'查看偏好与记忆',exact:true}).click();
  47 |  const receipt=page.waitForResponse(r=>r.request().postDataJSON()?.operation==='learning.recompute');await panel.getByRole('button',{name:'按当前边界重算',exact:true}).click();const body=await(await receipt).json();const id=body.result.job_id;expect(id).toBeTruthy();expect(body.result.state).toBe('queued');await expect(panel.getByRole('status')).toContainText(id);await expect.poll(()=>workers).toBe(1);
  48 |  await page.reload();await page.getByRole('link',{name:'记忆',exact:true}).click();await panel.getByRole('button',{name:'查询或继续重算任务',exact:true}).click();await expect(panel.getByRole('region',{name:'学习重算任务'})).toContainText(id);await panel.getByRole('button',{name:'取消重算',exact:true}).click();await expect(panel.getByRole('status')).toContainText('已取消');release();
  49 |  const call=async(operation:string,input:object={})=>(await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:member,...input}}})).json()).result;
  50 |  expect((await call('jobs.get',{id})).state).toBe('cancelled');expect((await call('learning.get')).inferred).toEqual([]);expect((await call('wear.list')).items).toEqual([]);
  51 |  }finally{release();await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});}
  52 | });
  53 | 
```