<?xml version="1.0" encoding="utf-8"?>
<CuttingControlProgram MeasurementSystem="Metric" Thickness="3">
  <Technology />
  <ApplicationFlat PlanWidth="800" PlanHeight="400.5" SheetWidth="1000" SheetHeight="500" />
  <SubPrograms>
    <SubProgram Id="1" Name="1" PartNumber="1" PartWidth="200" PartHeight="100">
      <ControlCommands>
        <SetOperationMode Id="2" Value="2" />
        <SetSensingSensor Id="3" Value="On" />
        <SetToolCorrection Id="4" Value="Right" />
        <MoveLineTo Id="5" X="78.5" Y="8.5" />
        <BeginCutOut Id="6" Contour="Unknown" />
        <LineTo Id="7" X="78.5" Y="20.5" />
        <LineTo Id="8" X="84.5" Y="20.5" />
        <LineTo Id="9" X="86.5" Y="18.5" />
        <LineTo Id="10" X="86.5" Y="10.5" />
        <LineTo Id="11" X="84.5" Y="8.5" />
        <LineTo Id="12" X="78.5" Y="8.5" />
        <EndCutOut Id="13" />
        <SetToolCorrection Id="14" Value="None" />
        <SetToolCorrection Id="15" Value="Right" />
        <MoveLineTo Id="16" X="98.5" Y="8.5" />
        <BeginCutOut Id="17" Contour="Unknown" />
        <LineTo Id="18" X="90.5" Y="8.5" />
        <LineTo Id="19" X="90.5" Y="20.5" />
        <LineTo Id="20" X="98.5" Y="20.5" />
        <EndCutOut Id="21" />
        <SetToolCorrection Id="22" Value="None" />
        <SetToolCorrection Id="23" Value="Right" />
        <MoveLineTo Id="24" X="90.5" Y="14.5" />
        <BeginCutOut Id="25" Contour="Unknown" />
        <LineTo Id="26" X="96.5" Y="14.5" />
        <EndCutOut Id="27" />
        <SetToolCorrection Id="28" Value="None" />
        <SetToolCorrection Id="29" Value="Right" />
        <MoveLineTo Id="30" X="102.5" Y="8.5" />
        <BeginCutOut Id="31" Contour="Unknown" />
        <LineTo Id="32" X="102.5" Y="20.5" />
        <LineTo Id="33" X="106.5" Y="14.5" />
        <LineTo Id="34" X="110.5" Y="20.5" />
        <LineTo Id="35" X="110.5" Y="8.5" />
        <EndCutOut Id="36" />
        <SetToolCorrection Id="37" Value="None" />
        <SetToolCorrection Id="38" Value="Right" />
        <MoveLineTo Id="39" X="116.5" Y="8.5" />
        <BeginCutOut Id="40" Contour="Unknown" />
        <LineTo Id="41" X="114.5" Y="10.5" />
        <LineTo Id="42" X="114.5" Y="18.5" />
        <LineTo Id="43" X="116.5" Y="20.5" />
        <LineTo Id="44" X="120.5" Y="20.5" />
        <LineTo Id="45" X="122.5" Y="18.5" />
        <LineTo Id="46" X="122.5" Y="10.5" />
        <LineTo Id="47" X="120.5" Y="8.5" />
        <LineTo Id="48" X="116.5" Y="8.5" />
        <EndCutOut Id="49" />
        <SetToolCorrection Id="50" Value="None" />
        <SetOperationMode Id="51" Value="3" />
        <SetToolCorrection Id="52" Value="Left" />
        <MoveLineTo Id="53" X="40.561720276" Y="50.561683167" />
        <BeginLeadIn Id="54" />
        <SetPiercingMode Id="55" Value="Fine" />
        <ExecPiercing Id="56" />
        <LineTo Id="57" X="34.984495817" Y="49.069067787" />
        <ArcTo Id="58" X="33.488526424" Y="43.492742034" SweepAngle="120" />
        <EndLeadIn Id="59" />
        <BeginCutOut Id="60" Contour="Interior" />
        <LineTo Id="61" X="34.733196779" Y="42.33030107" />
        <ArcTo Id="62" X="32.742887093" Y="44.189120557" SweepAngle="344.347826087" />
        <LineTo Id="63" X="33.488526424" Y="43.492742034" />
        <BeginLeadOut Id="64" />
        <LineTo Id="65" X="33.853944406" Y="43.151465462" />
        <ArcTo Id="66" X="39.348793278" Y="44.92346129" SweepAngle="120" />
        <LineTo Id="67" X="40.399135763" Y="49.811894645" />
        <EndLeadOut Id="68" />
        <EndCutOut Id="69" />
        <SetToolCorrection Id="70" Value="None" />
        <SetToolCorrection Id="71" Value="Left" />
        <MoveLineTo Id="72" X="104.8427" Y="55.3415" />
        <BeginLeadIn Id="73" />
        <SetPiercingMode Id="74" Value="Fine" />
        <ExecPiercing Id="75" />
        <LineTo Id="76" X="100.25" Y="55.3415" />
        <ArcTo Id="77" X="100.25" Y="45.5" SweepAngle="180" />
        <EndLeadIn Id="78" />
        <BeginCutOut Id="79" Contour="Interior" />
        <LineTo Id="80" X="120.5" Y="45.5" />
        <ArcTo Id="81" X="120.5" Y="55.5" SweepAngle="180" />
        <LineTo Id="82" X="80.5" Y="55.5" />
        <ArcTo Id="83" X="80.5" Y="45.5" SweepAngle="180" />
        <LineTo Id="84" X="100.25" Y="45.5" />
        <BeginLeadOut Id="85" />
        <LineTo Id="86" X="100.75" Y="45.5" />
        <ArcTo Id="87" X="100.75" Y="55.4" SweepAngle="180" />
        <LineTo Id="88" X="96.25" Y="55.4" />
        <EndLeadOut Id="89" />
        <EndCutOut Id="90" />
        <SetToolCorrection Id="91" Value="None" />
        <SetToolCorrection Id="92" Value="Left" />
        <MoveLineTo Id="93" X="160.561720276" Y="50.561683167" />
        <BeginLeadIn Id="94" />
        <SetPiercingMode Id="95" Value="Fine" />
        <ExecPiercing Id="96" />
        <LineTo Id="97" X="154.984495817" Y="49.069067787" />
        <ArcTo Id="98" X="153.488526424" Y="43.492742034" SweepAngle="120" />
        <EndLeadIn Id="99" />
        <BeginCutOut Id="100" Contour="Interior" />
        <LineTo Id="101" X="154.733196779" Y="42.33030107" />
        <ArcTo Id="102" X="152.742887093" Y="44.189120557" SweepAngle="344.347826087" />
        <LineTo Id="103" X="153.488526424" Y="43.492742034" />
        <BeginLeadOut Id="104" />
        <LineTo Id="105" X="153.853944406" Y="43.151465462" />
        <ArcTo Id="106" X="159.348793278" Y="44.92346129" SweepAngle="120" />
        <LineTo Id="107" X="160.399135763" Y="49.811894645" />
        <EndLeadOut Id="108" />
        <EndCutOut Id="109" />
        <SetToolCorrection Id="110" Value="None" />
        <SetToolCorrection Id="111" Value="Right" />
        <MoveLineTo Id="112" X="-6.5" Y="0.5" />
        <BeginLeadIn Id="113" />
        <SetPiercingMode Id="114" Value="Fine" />
        <ExecPiercing Id="115" />
        <LineTo Id="116" X="0.5" Y="0.5" />
        <EndLeadIn Id="117" />
        <BeginCutOut Id="118" Contour="Exterior" />
        <LineTo Id="119" X="200.5" Y="0.5" />
        <LineTo Id="120" X="200.5" Y="100.5" />
        <LineTo Id="121" X="0.5" Y="100.5" />
        <LineTo Id="122" X="0.5" Y="0.5" />
        <BeginLeadOut Id="123" />
        <LineTo Id="124" X="0.5" Y="-4.5" />
        <EndLeadOut Id="125" />
        <EndCutOut Id="126" />
        <SetToolCorrection Id="127" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="128" Name="2" PartNumber="2" PartWidth="200" PartHeight="100">
      <ControlCommands>
        <SetOperationMode Id="129" Value="3" />
        <SetToolCorrection Id="130" Value="Left" />
        <MoveLineTo Id="131" X="50" Y="49.5" />
        <BeginCutOut Id="132" Contour="Interior" />
        <ArcTo Id="133" X="49.629172873" Y="46.802032288" SweepAngle="344.347826087" />
        <LineTo Id="134" X="50" Y="49.5" />
        <EndCutOut Id="135" />
        <SetToolCorrection Id="136" Value="None" />
        <SetToolCorrection Id="137" Value="Left" />
        <MoveLineTo Id="138" X="80" Y="44.5" />
        <BeginCutOut Id="139" Contour="Interior" />
        <LineTo Id="140" X="120" Y="44.5" />
        <ArcTo Id="141" X="120" Y="54.5" SweepAngle="180" />
        <LineTo Id="142" X="80" Y="54.5" />
        <ArcTo Id="143" X="80" Y="44.5" SweepAngle="180" />
        <EndCutOut Id="144" />
        <SetToolCorrection Id="145" Value="None" />
        <SetToolCorrection Id="146" Value="Left" />
        <MoveLineTo Id="147" X="170" Y="49.5" />
        <BeginCutOut Id="148" Contour="Interior" />
        <ArcTo Id="149" X="169.629172873" Y="46.802032288" SweepAngle="344.347826087" />
        <LineTo Id="150" X="170" Y="49.5" />
        <EndCutOut Id="151" />
        <SetToolCorrection Id="152" Value="None" />
        <SetToolCorrection Id="153" Value="Right" />
        <MoveLineTo Id="154" X="0" Y="-0.5" />
        <BeginCutOut Id="155" Contour="Exterior" />
        <LineTo Id="156" X="200" Y="-0.5" />
        <LineTo Id="157" X="200" Y="99.5" />
        <LineTo Id="158" X="0" Y="99.5" />
        <LineTo Id="159" X="0" Y="-0.5" />
        <EndCutOut Id="160" />
        <SetToolCorrection Id="161" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="162" Name="3" PartNumber="3" PartWidth="200" PartHeight="100">
      <ControlCommands>
        <SetOperationMode Id="163" Value="3" />
        <SetToolCorrection Id="164" Value="Left" />
        <MoveLineTo Id="165" X="159.97137485" Y="49.471350907" />
        <BeginLeadIn Id="166" />
        <SetPiercingMode Id="167" Value="Fine" />
        <ExecPiercing Id="168" />
        <LineTo Id="169" X="165.54752504" Y="50.96797457" />
        <ArcTo Id="170" X="167.039486023" Y="56.545374121" SweepAngle="120" />
        <EndLeadIn Id="171" />
        <BeginCutOut Id="172" Contour="Interior" />
        <LineTo Id="173" X="166.825531432" Y="56.808359643" />
        <ArcTo Id="174" X="168.544194045" Y="54.6958395" SweepAngle="344.347826087" />
        <LineTo Id="175" X="167.039486023" Y="56.545374121" />
        <BeginLeadOut Id="176" />
        <ArcTo Id="177" X="161.199125857" Y="55.129334404" SweepAngle="128.364768711" />
        <LineTo Id="178" X="160.14865929" Y="50.240927712" />
        <EndLeadOut Id="179" />
        <EndCutOut Id="180" />
        <SetToolCorrection Id="181" Value="None" />
        <SetToolCorrection Id="182" Value="Left" />
        <MoveLineTo Id="183" X="95.6573" Y="44.6585" />
        <BeginLeadIn Id="184" />
        <SetPiercingMode Id="185" Value="Fine" />
        <ExecPiercing Id="186" />
        <LineTo Id="187" X="100.25" Y="44.6585" />
        <ArcTo Id="188" X="100.25" Y="54.5" SweepAngle="180" />
        <EndLeadIn Id="189" />
        <BeginCutOut Id="190" Contour="Interior" />
        <LineTo Id="191" X="80" Y="54.5" />
        <ArcTo Id="192" X="80" Y="44.5" SweepAngle="180" />
        <LineTo Id="193" X="120" Y="44.5" />
        <ArcTo Id="194" X="120" Y="54.5" SweepAngle="180" />
        <LineTo Id="195" X="100.25" Y="54.5" />
        <BeginLeadOut Id="196" />
        <LineTo Id="197" X="99.75" Y="54.5" />
        <ArcTo Id="198" X="99.75" Y="44.6" SweepAngle="180" />
        <LineTo Id="199" X="104.25" Y="44.6" />
        <EndLeadOut Id="200" />
        <EndCutOut Id="201" />
        <SetToolCorrection Id="202" Value="None" />
        <SetToolCorrection Id="203" Value="Left" />
        <MoveLineTo Id="204" X="39.97137485" Y="49.471350907" />
        <BeginLeadIn Id="205" />
        <SetPiercingMode Id="206" Value="Fine" />
        <ExecPiercing Id="207" />
        <LineTo Id="208" X="45.54752504" Y="50.96797457" />
        <ArcTo Id="209" X="47.039486023" Y="56.545374121" SweepAngle="120" />
        <EndLeadIn Id="210" />
        <BeginCutOut Id="211" Contour="Interior" />
        <LineTo Id="212" X="46.825531432" Y="56.808359643" />
        <ArcTo Id="213" X="48.544194045" Y="54.6958395" SweepAngle="344.347826087" />
        <LineTo Id="214" X="47.039486023" Y="56.545374121" />
        <BeginLeadOut Id="215" />
        <ArcTo Id="216" X="41.199125857" Y="55.129334404" SweepAngle="128.364768711" />
        <LineTo Id="217" X="40.14865929" Y="50.240927712" />
        <EndLeadOut Id="218" />
        <EndCutOut Id="219" />
        <SetToolCorrection Id="220" Value="None" />
        <SetToolCorrection Id="221" Value="Right" />
        <MoveLineTo Id="222" X="207" Y="99.5" />
        <BeginLeadIn Id="223" />
        <SetPiercingMode Id="224" Value="Fine" />
        <ExecPiercing Id="225" />
        <LineTo Id="226" X="200" Y="99.5" />
        <EndLeadIn Id="227" />
        <BeginCutOut Id="228" Contour="Exterior" />
        <LineTo Id="229" X="150" Y="99.5" />
        <ExecMicrojoint Id="230" />
        <LineTo Id="231" X="50" Y="99.5" />
        <ExecMicrojoint Id="232" />
        <LineTo Id="233" X="0" Y="99.5" />
        <LineTo Id="234" X="0" Y="49.5" />
        <ExecMicrojoint Id="235" />
        <LineTo Id="236" X="0" Y="-0.5" />
        <LineTo Id="237" X="50" Y="-0.5" />
        <ExecMicrojoint Id="238" />
        <LineTo Id="239" X="150" Y="-0.5" />
        <ExecMicrojoint Id="240" />
        <LineTo Id="241" X="200" Y="-0.5" />
        <LineTo Id="242" X="200" Y="49.5" />
        <ExecMicrojoint Id="243" />
        <LineTo Id="244" X="200" Y="99.5" />
        <BeginLeadOut Id="245" />
        <LineTo Id="246" X="200" Y="104.5" />
        <EndLeadOut Id="247" />
        <EndCutOut Id="248" />
        <SetToolCorrection Id="249" Value="None" />
      </ControlCommands>
    </SubProgram>
  </SubPrograms>
  <ControlCommands>
    <SetClamps Id="250" Value="Off" />
    <CallSubProgram Id="251" SubProgramName="1" X="50" Y="20" Rotation="0" />
    <CallSubProgram Id="252" SubProgramName="2" X="500" Y="50" Rotation="90" />
    <CallSubProgram Id="253" SubProgramName="3" X="800" Y="400" Rotation="180" />
  </ControlCommands>
</CuttingControlProgram>
