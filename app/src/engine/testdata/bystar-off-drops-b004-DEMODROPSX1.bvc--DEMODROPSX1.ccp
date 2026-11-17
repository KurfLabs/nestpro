<?xml version="1.0" encoding="utf-8"?>
<CuttingControlProgram MeasurementSystem="Metric" Thickness="3">
  <Technology />
  <ApplicationFlat PlanWidth="3000.02" PlanHeight="1000.02" SheetWidth="3000" SheetHeight="1000" />
  <SubPrograms>
    <SubProgram Id="1" Name="1" PartNumber="1" PartWidth="400" PartHeight="400">
      <ControlCommands>
        <SetOperationMode Id="2" Value="3" />
        <SetSensingSensor Id="3" Value="On" />
        <SetToolCorrection Id="4" Value="Right" />
        <MoveLineTo Id="5" X="407" Y="400" />
        <BeginLeadIn Id="6" />
        <SetPiercingMode Id="7" Value="Fine" />
        <ExecPiercing Id="8" />
        <LineTo Id="9" X="400" Y="400" />
        <EndLeadIn Id="10" />
        <BeginCutOut Id="11" Contour="Exterior" />
        <LineTo Id="12" X="0" Y="400" />
        <LineTo Id="13" X="0" Y="0" />
        <LineTo Id="14" X="400" Y="0" />
        <LineTo Id="15" X="400" Y="400" />
        <BeginLeadOut Id="16" />
        <LineTo Id="17" X="400" Y="405" />
        <EndLeadOut Id="18" />
        <EndCutOut Id="19" />
        <SetToolCorrection Id="20" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="21" Name="2" PartNumber="1" PartWidth="400" PartHeight="400">
      <ControlCommands>
        <SetOperationMode Id="22" Value="3" />
        <SetToolCorrection Id="23" Value="Right" />
        <MoveLineTo Id="24" X="407" Y="400" />
        <BeginLeadIn Id="25" />
        <SetPiercingMode Id="26" Value="Fine" />
        <ExecPiercing Id="27" />
        <LineTo Id="28" X="400" Y="400" />
        <EndLeadIn Id="29" />
        <BeginCutOut Id="30" Contour="Exterior" />
        <LineTo Id="31" X="0" Y="400" />
        <LineTo Id="32" X="0" Y="0" />
        <LineTo Id="33" X="400" Y="0" />
        <LineTo Id="34" X="400" Y="400" />
        <BeginLeadOut Id="35" />
        <LineTo Id="36" X="400" Y="405" />
        <EndLeadOut Id="37" />
        <EndCutOut Id="38" />
        <SetToolCorrection Id="39" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="40" Name="3" PartNumber="1" PartWidth="400" PartHeight="400">
      <ControlCommands>
        <SetOperationMode Id="41" Value="3" />
        <SetToolCorrection Id="42" Value="Right" />
        <MoveLineTo Id="43" X="407" Y="80" />
        <BeginLeadIn Id="44" />
        <SetPiercingMode Id="45" Value="Fine" />
        <ExecPiercing Id="46" />
        <LineTo Id="47" X="400" Y="80" />
        <EndLeadIn Id="48" />
        <BeginCutOut Id="49" Contour="Exterior" />
        <LineTo Id="50" X="400" Y="400" />
        <LineTo Id="51" X="0" Y="400" />
        <LineTo Id="52" X="0" Y="0" />
        <LineTo Id="53" X="400" Y="0" />
        <LineTo Id="54" X="400" Y="80" />
        <BeginLeadOut Id="55" />
        <LineTo Id="56" X="400" Y="80.5" />
        <LineTo Id="57" X="405" Y="80.5" />
        <EndLeadOut Id="58" />
        <EndCutOut Id="59" />
        <SetToolCorrection Id="60" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="61" Name="ResidualSheetCuts" PartWidth="2001.07" PartHeight="1000.04">
      <ControlCommands>
        <SetOperationMode Id="62" Value="0" />
        <SetToolCorrection Id="63" Value="None" />
        <SetToolCorrection Id="64" Value="Right" />
        <MoveLineTo Id="65" X="2999" Y="500.15" />
        <BeginCutOut Id="66" Contour="Exterior" />
        <SetPiercingMode Id="67" Value="Fine" />
        <ExecPiercing Id="68" />
        <SetSensingSensor Id="69" Value="Off" />
        <LineTo Id="70" X="3000.02" Y="500.15" />
        <SetSensingSensor Id="71" Value="On" />
        <EndCutOut Id="72" />
        <ExecHighPositioning Id="73" />
        <MoveLineTo Id="74" X="2999" Y="500.15" />
        <BeginCutOut Id="75" Contour="Exterior" />
        <SetPiercingMode Id="76" Value="NoTime" />
        <ExecPiercing Id="77" />
        <LineTo Id="78" X="998.95" Y="500.15" />
        <EndCutOut Id="79" />
        <ExecHighPositioning Id="80" />
        <SetToolCorrection Id="81" Value="None" />
        <SetOperationMode Id="82" Value="0" />
        <SetToolCorrection Id="83" Value="None" />
        <SetToolCorrection Id="84" Value="Right" />
        <MoveLineTo Id="85" X="998.95" Y="1" />
        <BeginCutOut Id="86" Contour="Exterior" />
        <SetPiercingMode Id="87" Value="Fine" />
        <ExecPiercing Id="88" />
        <SetSensingSensor Id="89" Value="Off" />
        <LineTo Id="90" X="998.95" Y="-0.02" />
        <SetSensingSensor Id="91" Value="On" />
        <EndCutOut Id="92" />
        <ExecHighPositioning Id="93" />
        <MoveLineTo Id="94" X="998.95" Y="1" />
        <BeginCutOut Id="95" Contour="Exterior" />
        <SetPiercingMode Id="96" Value="NoTime" />
        <ExecPiercing Id="97" />
        <LineTo Id="98" X="998.95" Y="999.02" />
        <SetSensingSensor Id="99" Value="Off" />
        <LineTo Id="100" X="998.95" Y="1000.02" />
        <SetSensingSensor Id="101" Value="On" />
        <EndCutOut Id="102" />
        <ExecHighPositioning Id="103" />
        <SetToolCorrection Id="104" Value="None" />
        <SetOperationMode Id="105" Value="0" />
        <SetToolCorrection Id="106" Value="None" />
        <SetToolCorrection Id="107" Value="Right" />
        <MoveLineTo Id="108" X="1499.25" Y="1" />
        <BeginCutOut Id="109" Contour="Exterior" />
        <SetPiercingMode Id="110" Value="Fine" />
        <ExecPiercing Id="111" />
        <SetSensingSensor Id="112" Value="Off" />
        <LineTo Id="113" X="1499.25" Y="-0.02" />
        <SetSensingSensor Id="114" Value="On" />
        <EndCutOut Id="115" />
        <ExecHighPositioning Id="116" />
        <MoveLineTo Id="117" X="1499.25" Y="1" />
        <BeginCutOut Id="118" Contour="Exterior" />
        <SetPiercingMode Id="119" Value="NoTime" />
        <ExecPiercing Id="120" />
        <LineTo Id="121" X="1499.25" Y="999.02" />
        <SetSensingSensor Id="122" Value="Off" />
        <LineTo Id="123" X="1499.25" Y="1000.02" />
        <SetSensingSensor Id="124" Value="On" />
        <EndCutOut Id="125" />
        <ExecHighPositioning Id="126" />
        <SetToolCorrection Id="127" Value="None" />
        <SetOperationMode Id="128" Value="0" />
        <SetToolCorrection Id="129" Value="None" />
        <SetToolCorrection Id="130" Value="Right" />
        <MoveLineTo Id="131" X="1999.55" Y="1" />
        <BeginCutOut Id="132" Contour="Exterior" />
        <SetPiercingMode Id="133" Value="Fine" />
        <ExecPiercing Id="134" />
        <SetSensingSensor Id="135" Value="Off" />
        <LineTo Id="136" X="1999.55" Y="-0.02" />
        <SetSensingSensor Id="137" Value="On" />
        <EndCutOut Id="138" />
        <ExecHighPositioning Id="139" />
        <MoveLineTo Id="140" X="1999.55" Y="1" />
        <BeginCutOut Id="141" Contour="Exterior" />
        <SetPiercingMode Id="142" Value="NoTime" />
        <ExecPiercing Id="143" />
        <LineTo Id="144" X="1999.55" Y="999.02" />
        <SetSensingSensor Id="145" Value="Off" />
        <LineTo Id="146" X="1999.55" Y="1000.02" />
        <SetSensingSensor Id="147" Value="On" />
        <EndCutOut Id="148" />
        <ExecHighPositioning Id="149" />
        <SetToolCorrection Id="150" Value="None" />
        <SetOperationMode Id="151" Value="0" />
        <SetToolCorrection Id="152" Value="None" />
        <SetToolCorrection Id="153" Value="Right" />
        <MoveLineTo Id="154" X="2499.85" Y="1" />
        <BeginCutOut Id="155" Contour="Exterior" />
        <SetPiercingMode Id="156" Value="Fine" />
        <ExecPiercing Id="157" />
        <SetSensingSensor Id="158" Value="Off" />
        <LineTo Id="159" X="2499.85" Y="-0.02" />
        <SetSensingSensor Id="160" Value="On" />
        <EndCutOut Id="161" />
        <ExecHighPositioning Id="162" />
        <MoveLineTo Id="163" X="2499.85" Y="1" />
        <BeginCutOut Id="164" Contour="Exterior" />
        <SetPiercingMode Id="165" Value="NoTime" />
        <ExecPiercing Id="166" />
        <LineTo Id="167" X="2499.85" Y="999.02" />
        <SetSensingSensor Id="168" Value="Off" />
        <LineTo Id="169" X="2499.85" Y="1000.02" />
        <SetSensingSensor Id="170" Value="On" />
        <EndCutOut Id="171" />
        <ExecHighPositioning Id="172" />
        <SetToolCorrection Id="173" Value="None" />
      </ControlCommands>
    </SubProgram>
  </SubPrograms>
  <ControlCommands>
    <SetClamps Id="174" Value="Off" />
    <CallSubProgram Id="175" SubProgramName="1" X="5" Y="5" Rotation="0" />
    <CallSubProgram Id="176" SubProgramName="2" X="5" Y="410" Rotation="0" />
    <CallSubProgram Id="177" SubProgramName="3" X="410" Y="5" Rotation="0" />
    <CallSubProgram Id="178" SubProgramName="3" X="410" Y="410" Rotation="0" />
    <CallSubProgram Id="179" SubProgramName="ResidualSheetCuts" X="0" Y="0" Rotation="0" />
  </ControlCommands>
</CuttingControlProgram>
