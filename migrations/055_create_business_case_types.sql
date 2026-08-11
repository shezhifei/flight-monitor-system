-- Migration: 055_create_business_case_types
-- Description: 新建业务事项类型定义表 (business_case_types)
--
-- Up:

CREATE TABLE IF NOT EXISTS business_case_types (
    id         VARCHAR(26) PRIMARY KEY,
    code       VARCHAR(64) NOT NULL UNIQUE,
    name       VARCHAR(100) NOT NULL,
    bpmn_xml   TEXT,
    description TEXT,
    is_active  BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE  business_case_types IS '业务事项类型（关联 Flowable 流程编排）';
COMMENT ON COLUMN business_case_types.code IS '唯一编码，同时作为 Flowable process key';
COMMENT ON COLUMN business_case_types.bpmn_xml IS '该事项关联的 BPMN 流程 XML 草稿';

--
-- Down:
-- DROP TABLE IF EXISTS business_case_types;
