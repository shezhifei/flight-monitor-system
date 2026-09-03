from src.application.dto.dispatch_dtos import Department, Equipment, Team


def test_department_dto():
    d = Department(id="1", name="运行部", code="OPS")
    assert d.id == "1"


def test_team_dto():
    t = Team(id="1", name="一队", department_id="OPS")
    assert t.name == "一队"


def test_equipment_dto():
    e = Equipment(id="1", name="牵引车A", type="tractor")
    assert e.type == "tractor"
