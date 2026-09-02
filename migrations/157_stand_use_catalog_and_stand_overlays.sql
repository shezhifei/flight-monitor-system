-- 157（切片 B）：机位 schema 真正元数据驱动。
-- 1) 封闭有序码表 stand_use：near_domestic / near_international / remote。
-- 2) Stand 六项扩展字段 overlay（值落 stands.attributes JSONB，不 ALTER 业务列）。
-- 约束：无物理 FK（120 之后），引用完整性由应用层 enforce；
-- ON CONFLICT DO NOTHING —— 不做任何改 field_type 的后续 UPDATE。

INSERT INTO metadata_catalogs (code, name, description, is_open, is_ordered, system_owned)
VALUES
    (
        'stand_use',
        '机位用途',
        '封闭有序码表：近机位国内 / 近机位国际 / 远机位。远机位不得配对应登机口（visible_when）。',
        FALSE,
        TRUE,
        TRUE
    )
ON CONFLICT (code) DO NOTHING;

INSERT INTO metadata_catalog_entries (catalog_code, code, name, rank, source)
VALUES
    ('stand_use', 'near_domestic', '近机位（国内）', 1, 'manual'),
    ('stand_use', 'near_international', '近机位（国际）', 2, 'manual'),
    ('stand_use', 'remote', '远机位', 3, 'manual')
ON CONFLICT (catalog_code, code) DO NOTHING;

INSERT INTO ontology_field_overlays
    (object_name, field_name, field_type, catalog_code, object_name_target, description, visible_when)
VALUES
    (
        'Stand', 'max_size_category', 'catalog_ref', 'icao_size', NULL,
        '最大 ICAO 等级', NULL
    ),
    (
        'Stand', 'combined_stand', 'boolean', NULL, NULL,
        '是否组合机位', NULL
    ),
    (
        'Stand', 'stand_use', 'catalog_ref', 'stand_use', NULL,
        '机位用途', NULL
    ),
    (
        'Stand', 'composed_of', 'object_ref[]', NULL, 'Stand',
        '组成子机位',
        '{"field": "combined_stand", "op": "eq", "value": true}'::jsonb
    ),
    (
        'Stand', 'corresponding_gate', 'object_ref', NULL, 'Gate',
        '对应登机口',
        '{"field": "stand_use", "op": "neq", "value": "remote"}'::jsonb
    )
ON CONFLICT (object_name, field_name) DO NOTHING;

COMMENT ON COLUMN ontology_field_overlays.visible_when IS
    '{ field, op, value }（op 缺省 eq）；不满足时表单隐藏且服务端丢弃该字段值';
